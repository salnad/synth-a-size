// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Sample` (a loaded sample with band-limited octave copies) and
//! `vital::SampleSource` (the sample player).
//!
//! Ports `vital/src/synthesis/producers/sample_source.{h,cpp}`.
//!
//! Upstream double-buffers the sample data between the loading thread and
//! the audio thread (`current_data_`/`active_audio_data_`). Here the
//! [`Sample`] owns one [`SampleData`], shared through [`SharedSample`]; the
//! engine decides how to hand a new sample to the audio thread.
//!
//! Each band-limited buffer keeps upstream's layout: `kBufferSamples` (4)
//! samples of padding (zeros, or the wrapped-around end for loop buffers)
//! on each side of the audio. The player reads them from the start of the
//! padding, as upstream does.

use std::cell::RefCell;
use std::rc::Rc;

use sas_common::base64;
use sas_common::json::{self, Value};
use sas_common::random::RandomGenerator;
use sas_common::scalar::{float_to_pcm, pcm_to_float};
use sas_dsp_core::{constants, Output, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::{CENTS_PER_NOTE, DEFAULT_SAMPLE_RATE, MIDI_TRACK_CENTER};
use sas_simd::conversions::float_to_i32;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

/// A sample shared by a player and its voice copies (`std::shared_ptr<Sample>`).
pub type SharedSample = Rc<RefCell<Sample>>;

// Input indices.
pub const RESET: usize = 0;
pub const MIDI: usize = 1;
pub const KEYTRACK: usize = 2;
pub const LEVEL: usize = 3;
pub const RANDOM_PHASE: usize = 4;
pub const TRANSPOSE: usize = 5;
pub const TRANSPOSE_QUANTIZE: usize = 6;
pub const TUNE: usize = 7;
pub const LOOP: usize = 8;
pub const BOUNCE: usize = 9;
pub const PAN: usize = 10;
pub const NOTE_COUNT: usize = 11;
pub const NUM_INPUTS: usize = 12;

// Output indices.
pub const RAW: usize = 0;
pub const LEVELLED: usize = 1;
pub const NUM_OUTPUTS: usize = 2;

pub const MAX_TRANSPOSE: f32 = 96.0;
pub const MIN_TRANSPOSE: f32 = -96.0;
pub const MAX_AMPLITUDE: f32 = 1.414_213_562_37;
pub const NUM_DOWNSAMPLE_TAPS: usize = 55;
pub const NUM_UPSAMPLE_TAPS: usize = 52;

const DEFAULT_NAME: &str = "White Noise";

#[rustfmt::skip]
const UPSAMPLE_COEFFICIENTS: [f32; NUM_UPSAMPLE_TAPS] = [
    -0.000159813115702086552469274316479186382,
    0.000225405365781280835058009159865832771,
    -0.000378616814007205900686342525673921955,
    0.000594907533596884547516525643118256994,
    -0.000890530515941817101682742574553230952,
    0.001284040046393844676508866342601322685,
    -0.001796543223638378920792302295694753411,
    0.002451862103068884121692683208948437823,
    -0.003276873018553504678107568537370752892,
    0.004302012661141991003987961050825106213,
    -0.005561976429934398571952591794342879439,
    0.007097105459677621741576558633823879063,
    -0.008955232561651555595050311353588767815,
    0.011195057708851860467369476737076183781,
    -0.013890548104646217864033275191104621626,
    0.017139719620821350365424962092220084742,
    -0.021077036318492142763503238711564335972,
    0.025897497908177177783350941808748757467,
    -0.03189749744607761616776997470878995955,
    0.039555400754278852160084056777122896165,
    -0.049699764879031965714162311087420675904,
    0.063901297378209126476278356676630210131,
    -0.08553732517833501081128133591846562922,
    0.123410206086688845061871688812971115112,
    -0.209837893291539345774765479291090741754,
    0.63582677174146173815216798175242729485,
    0.63582677174146173815216798175242729485,
    -0.209837893291539345774765479291090741754,
    0.123410206086688845061871688812971115112,
    -0.08553732517833501081128133591846562922,
    0.063901297378209126476278356676630210131,
    -0.049699764879031965714162311087420675904,
    0.039555400754278852160084056777122896165,
    -0.03189749744607761616776997470878995955,
    0.025897497908177177783350941808748757467,
    -0.021077036318492142763503238711564335972,
    0.017139719620821350365424962092220084742,
    -0.013890548104646217864033275191104621626,
    0.011195057708851860467369476737076183781,
    -0.008955232561651555595050311353588767815,
    0.007097105459677621741576558633823879063,
    -0.005561976429934398571952591794342879439,
    0.004302012661141991003987961050825106213,
    -0.003276873018553504678107568537370752892,
    0.002451862103068884121692683208948437823,
    -0.001796543223638378920792302295694753411,
    0.001284040046393844676508866342601322685,
    -0.000890530515941817101682742574553230952,
    0.000594907533596884547516525643118256994,
    -0.000378616814007205900686342525673921955,
    0.000225405365781280835058009159865832771,
    -0.000159813115702086552469274316479186382,
];

#[rustfmt::skip]
const DOWNSAMPLE_COEFFICIENTS: [f32; NUM_DOWNSAMPLE_TAPS] = [
    -0.0013796309221920304,
    -0.0008322130675804714,
    0.0030100376204235577,
    0.00666031332700994,
    0.0040620073330527315,
    -0.003019073425031439,
    -0.004450269579432283,
    0.0030526281279541555,
    0.007614361286489334,
    -0.000546514301955849,
    -0.010099270019478761,
    -0.003465846383906444,
    0.011760981765402261,
    0.009402148654924303,
    -0.011429260748035207,
    -0.016935843679984037,
    0.008026778073943279,
    0.025557280950428782,
    -0.0002093220301655805,
    -0.03448379812688787,
    -0.013983156365753766,
    0.04279770831566429,
    0.03889228625534586,
    -0.049566024787935245,
    -0.09025827224454164,
    0.05398926693924448,
    0.31285587793730246,
    0.4444714418837066,
    0.31285587793730246,
    0.05398926693924448,
    -0.09025827224454164,
    -0.049566024787935245,
    0.03889228625534586,
    0.04279770831566429,
    -0.013983156365753766,
    -0.03448379812688787,
    -0.0002093220301655805,
    0.025557280950428782,
    0.008026778073943279,
    -0.016935843679984037,
    -0.011429260748035207,
    0.009402148654924303,
    0.011760981765402261,
    -0.003465846383906444,
    -0.010099270019478761,
    -0.000546514301955849,
    0.007614361286489334,
    0.0030526281279541555,
    -0.004450269579432283,
    -0.003019073425031439,
    0.0040620073330527315,
    0.00666031332700994,
    0.0030100376204235577,
    -0.0008322130675804714,
    -0.0013796309221920304,
];

fn get_filtered_sample(buffer: &[f32], index: i32, size: i32) -> f32 {
    let radius = NUM_DOWNSAMPLE_TAPS as i32 / 2;
    let start = 0.max(index - radius);
    let end = (size - 1).min(index + radius);
    let mut total = 0.0f32;
    for i in start..=end {
        let coefficient = DOWNSAMPLE_COEFFICIENTS[(i - index + radius) as usize];
        total += coefficient * buffer[i as usize];
    }
    total
}

fn get_filtered_loop_sample(buffer: &[f32], index: i32, size: i32) -> f32 {
    let radius = NUM_DOWNSAMPLE_TAPS as i32 / 2;
    let start = index - radius;
    let end = index + radius;
    let mut total = 0.0f32;
    for i in start..=end {
        let buffer_index = (i + size * radius) % size;
        let coefficient = DOWNSAMPLE_COEFFICIENTS[(i - index + radius) as usize];
        total += coefficient * buffer[buffer_index as usize];
    }
    total
}

fn get_interpolated_sample(buffer: &[f32], index: i32, size: i32) -> f32 {
    let radius = NUM_UPSAMPLE_TAPS as i32 / 2;
    let start = 0.max(index - radius + 1);
    let end = (size - 1).min(index + radius);
    let mut total = 0.0f32;
    for i in start..=end {
        let coefficient_index = (i - index + radius - 1) as usize;
        total += UPSAMPLE_COEFFICIENTS[coefficient_index] * buffer[i as usize];
    }
    total
}

fn upsample(original: &[f32], dest: &mut [f32], original_size: usize) {
    for i in 0..original_size {
        let value1 = original[i];
        let value2 = get_interpolated_sample(original, i as i32, original_size as i32);
        dest[2 * i] = value1;
        dest[2 * i + 1] = value2;
    }
}

fn downsample(original: &[f32], dest: &mut [f32], original_size: usize, dest_size: usize) {
    for (i, value) in dest.iter_mut().enumerate().take(dest_size) {
        *value = get_filtered_sample(original, 2 * i as i32, original_size as i32);
    }
}

fn downsample_loop(original: &[f32], dest: &mut [f32], original_size: usize, dest_size: usize) {
    for (i, value) in dest.iter_mut().enumerate().take(dest_size) {
        *value = get_filtered_loop_sample(original, 2 * i as i32, original_size as i32);
    }
}

/// `createBandLimitedBuffers`: the upsampled copy first, then the original,
/// then halvings down to fewer than `kMinSize` samples.
fn create_band_limited_buffers(
    destination: &mut Vec<Vec<f32>>,
    loop_destination: &mut Vec<Vec<f32>>,
    buffer: &[f32],
    size: usize,
) {
    const PAD: usize = Sample::BUFFER_SAMPLES;
    let mut play_buffer = vec![0.0f32; size + 2 * PAD];
    let mut loop_buffer = vec![0.0f32; size + 2 * PAD];
    play_buffer[PAD..PAD + size].copy_from_slice(&buffer[..size]);
    loop_buffer[PAD..PAD + size].copy_from_slice(&buffer[..size]);

    for i in 0..PAD {
        play_buffer[i] = 0.0;
        play_buffer[size + PAD + i] = 0.0;

        loop_buffer[i] = loop_buffer[size + i];
        loop_buffer[size + PAD + i] = loop_buffer[PAD + i];
    }
    destination.push(play_buffer);
    loop_destination.push(loop_buffer);

    let mut current_size = size;
    for _ in 0..Sample::UPSAMPLE_TIMES {
        let upsampled_size = current_size * 2;
        let num_samples = upsampled_size + 2 * PAD;
        let mut next_buffer = vec![0.0f32; num_samples];
        upsample(
            &destination[destination.len() - 1][PAD..],
            &mut next_buffer[PAD..],
            current_size,
        );
        let next_loop_buffer = next_buffer.clone();
        destination.insert(0, next_buffer);
        loop_destination.insert(0, next_loop_buffer);
        current_size = upsampled_size;
    }

    current_size = size;
    while current_size >= Sample::MIN_SIZE {
        let next_size = current_size.div_ceil(2);
        let mut next_buffer = vec![0.0f32; next_size + 2 * PAD];
        let mut next_loop_buffer = vec![0.0f32; next_size + 2 * PAD];
        {
            let play_buffer = destination.last().unwrap();
            let loop_buffer = loop_destination.last().unwrap();
            downsample(
                &play_buffer[PAD..],
                &mut next_buffer[PAD..],
                current_size,
                next_size,
            );
            downsample_loop(
                &loop_buffer[PAD..],
                &mut next_loop_buffer[PAD..],
                current_size,
                next_size,
            );

            for i in 0..PAD {
                next_buffer[i] = 0.0;
                next_buffer[next_size + PAD + i] = 0.0;

                next_loop_buffer[i] = loop_buffer[next_size + i];
                next_loop_buffer[next_size + PAD + i] = next_loop_buffer[PAD + i];
            }
        }
        destination.push(next_buffer);
        loop_destination.push(next_loop_buffer);
        current_size = next_size;
    }
}

/// `Sample::SampleData`.
#[derive(Clone, Debug)]
pub struct SampleData {
    pub length: usize,
    pub sample_rate: i32,
    pub stereo: bool,
    pub left_buffers: Vec<Vec<f32>>,
    pub left_loop_buffers: Vec<Vec<f32>>,
    pub right_buffers: Vec<Vec<f32>>,
    pub right_loop_buffers: Vec<Vec<f32>>,
}

/// `vital::Sample`.
#[derive(Clone, Debug)]
pub struct Sample {
    name: String,
    last_browsed_file: String,
    data: SampleData,
}

impl Sample {
    pub const DEFAULT_SAMPLE_LENGTH: usize = 44100;
    pub const UPSAMPLE_TIMES: usize = 1;
    pub const BUFFER_SAMPLES: usize = 4;
    pub const MIN_SIZE: usize = 4;
    /// `kMaxSize` of the mono `loadSample`.
    pub const MAX_SIZE: usize = 1_764_000;

    /// `Sample()`: the white-noise default (takes a seed from the global
    /// counter, see [`Sample::init`]).
    pub fn new() -> Self {
        let mut sample = Self {
            name: DEFAULT_NAME.to_string(),
            last_browsed_file: String::new(),
            data: SampleData {
                length: 0,
                sample_rate: 0,
                stereo: false,
                left_buffers: Vec::new(),
                left_loop_buffers: Vec::new(),
                right_buffers: Vec::new(),
                right_loop_buffers: Vec::new(),
            },
        };
        sample.init();
        sample
    }

    /// `init()`: 44100 samples of white noise in `[-0.9, 0.9)` at 44.1 kHz.
    pub fn init(&mut self) {
        self.name = DEFAULT_NAME.to_string();
        let mut random_generator = RandomGenerator::from_global_counter(-0.9, 0.9);
        let buffer: Vec<f32> = (0..Self::DEFAULT_SAMPLE_LENGTH)
            .map(|_| random_generator.next())
            .collect();
        self.load_sample(&buffer, Self::DEFAULT_SAMPLE_LENGTH, DEFAULT_SAMPLE_RATE);
    }

    /// `loadSample(buffer, size, sample_rate)` (mono; at most
    /// [`Sample::MAX_SIZE`] samples).
    pub fn load_sample(&mut self, buffer: &[f32], size: usize, sample_rate: i32) {
        let size = size.min(Self::MAX_SIZE);
        let mut data = SampleData {
            length: size,
            sample_rate,
            stereo: false,
            left_buffers: Vec::new(),
            left_loop_buffers: Vec::new(),
            right_buffers: Vec::new(),
            right_loop_buffers: Vec::new(),
        };
        create_band_limited_buffers(
            &mut data.left_buffers,
            &mut data.left_loop_buffers,
            buffer,
            size,
        );
        self.data = data;
    }

    /// `loadSample(left, right, size, sample_rate)`.
    pub fn load_stereo_sample(
        &mut self,
        left: &[f32],
        right: &[f32],
        size: usize,
        sample_rate: i32,
    ) {
        let mut data = SampleData {
            length: size,
            sample_rate,
            stereo: true,
            left_buffers: Vec::new(),
            left_loop_buffers: Vec::new(),
            right_buffers: Vec::new(),
            right_loop_buffers: Vec::new(),
        };
        create_band_limited_buffers(
            &mut data.left_buffers,
            &mut data.left_loop_buffers,
            left,
            size,
        );
        create_band_limited_buffers(
            &mut data.right_buffers,
            &mut data.right_loop_buffers,
            right,
            size,
        );
        self.data = data;
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_last_browsed_file(&mut self, path: &str) {
        self.last_browsed_file = path.to_string();
    }

    pub fn last_browsed_file(&self) -> &str {
        &self.last_browsed_file
    }

    pub fn data(&self) -> &SampleData {
        &self.data
    }

    pub fn original_length(&self) -> usize {
        self.data.length
    }

    pub fn upsample_length(&self) -> usize {
        self.original_length() * (1 << Self::UPSAMPLE_TIMES)
    }

    pub fn sample_rate(&self) -> i32 {
        self.data.sample_rate
    }

    /// `activeLength()`, in upsampled samples.
    pub fn active_length(&self) -> usize {
        self.data.length * (1 << Self::UPSAMPLE_TIMES)
    }

    pub fn active_sample_rate(&self) -> i32 {
        self.data.sample_rate
    }

    /// `buffer()`: the original-rate audio, without padding.
    pub fn buffer(&self) -> &[f32] {
        &self.data.left_buffers[Self::UPSAMPLE_TIMES][1..]
    }

    /// `getActiveIndex(delta)`: the octave copy to play at `delta` samples
    /// per output sample.
    pub fn active_index(&self, delta: f32) -> usize {
        let octaves = sas_simd::utils::ilog2(float_to_i32(delta).max(1));
        octaves.min(self.data.left_buffers.len() as i32 - 1) as usize
    }

    pub fn active_left_buffer(&self, index: usize) -> &[f32] {
        &self.data.left_buffers[index]
    }

    pub fn active_left_loop_buffer(&self, index: usize) -> &[f32] {
        &self.data.left_loop_buffers[index]
    }

    pub fn active_right_buffer(&self, index: usize) -> &[f32] {
        if self.data.stereo {
            &self.data.right_buffers[index]
        } else {
            self.active_left_buffer(index)
        }
    }

    pub fn active_right_loop_buffer(&self, index: usize) -> &[f32] {
        if self.data.stereo {
            &self.data.right_loop_buffers[index]
        } else {
            self.active_left_loop_buffer(index)
        }
    }

    /// `stateToJson()`.
    pub fn state_to_json(&self) -> Value {
        let mut data = Value::object();
        data.set("name", self.name.as_str());
        data.set("length", self.data.length as i64);
        data.set("sample_rate", self.data.sample_rate as i64);
        let original = Self::UPSAMPLE_TIMES;
        let pcm = float_to_pcm(&self.data.left_buffers[original][..self.data.length]);
        data.set("samples", base64::encode_i16s(&pcm));
        if self.data.stereo {
            let pcm = float_to_pcm(&self.data.right_buffers[original][..self.data.length]);
            data.set("samples_stereo", base64::encode_i16s(&pcm));
        }
        data
    }

    /// `jsonToState(data)`.
    pub fn json_to_state(&mut self, data: &Value) -> json::Result<()> {
        self.name = String::new();
        if data.count("name") != 0 {
            self.name = data.get("name")?.to_str()?.to_string();
        }

        let length = data.get("length")?.to_i32()? as usize;
        let sample_rate = data.get("sample_rate")?.to_i32()?;

        let decode = |key: &str| -> json::Result<Vec<f32>> {
            let mut pcm = base64::decode_i16s(data.get(key)?.to_str()?);
            pcm.resize(length, 0);
            Ok(pcm_to_float(&pcm))
        };
        let buffer = decode("samples")?;
        if data.count("samples_stereo") != 0 {
            let buffer_stereo = decode("samples_stereo")?;
            self.load_stereo_sample(&buffer, &buffer_stereo, length, sample_rate);
        } else {
            self.load_sample(&buffer, length, sample_rate);
        }
        Ok(())
    }
}

impl Default for Sample {
    fn default() -> Self {
        Self::new()
    }
}

/// `vital::SampleSource`'s members.
pub struct SampleSource {
    pan_amplitude: PolyFloat,
    transpose_quantize: i32,
    last_quantized_transpose: PolyFloat,
    sample_index: PolyFloat,
    sample_fraction: PolyFloat,
    phase_inc: PolyFloat,
    bounce_mask: PolyMask,
    phase_output: Rc<Output>,
    random_generator: RandomGenerator,
    sample: SharedSample,
}

impl Clone for SampleSource {
    /// The copy constructor: shares the sample and the phase output; the
    /// generator takes a fresh seed, as every upstream `RandomGenerator`
    /// copy does.
    fn clone(&self) -> Self {
        Self {
            pan_amplitude: self.pan_amplitude,
            transpose_quantize: self.transpose_quantize,
            last_quantized_transpose: self.last_quantized_transpose,
            sample_index: self.sample_index,
            sample_fraction: self.sample_fraction,
            phase_inc: self.phase_inc,
            bounce_mask: self.bounce_mask,
            phase_output: self.phase_output.clone(),
            random_generator: RandomGenerator::from_global_counter(
                self.random_generator.min(),
                self.random_generator.max(),
            ),
            sample: self.sample.clone(),
        }
    }
}

impl SampleSource {
    /// `SampleSource()` (the kernel; see [`SampleSource::create`]). Takes two
    /// seeds from the global counter: the player's generator, then the
    /// default sample's noise.
    pub fn new() -> Self {
        let random_generator = RandomGenerator::from_global_counter(0.0, 1.0);
        let sample = Rc::new(RefCell::new(Sample::new()));
        Self {
            pan_amplitude: PolyFloat::zero(),
            transpose_quantize: 0,
            last_quantized_transpose: PolyFloat::zero(),
            sample_index: PolyFloat::zero(),
            sample_fraction: PolyFloat::zero(),
            phase_inc: PolyFloat::zero(),
            bounce_mask: PolyMask::zero(),
            phase_output: Output::new_control_rate(),
            random_generator,
            sample,
        }
    }

    /// `new SampleSource()`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), NUM_INPUTS, NUM_OUTPUTS, false, 1)
    }

    /// `getSample()`.
    pub fn sample(&self) -> &SharedSample {
        &self.sample
    }

    /// `getPhaseOutput()`.
    pub fn phase_output(&self) -> &Rc<Output> {
        &self.phase_output
    }

    /// `phase_inc_`: the playback increment of the last processed block, in
    /// upsampled sample positions per output sample.
    pub fn phase_inc(&self) -> PolyFloat {
        self.phase_inc
    }

    fn snap_transpose(
        &mut self,
        input_midi: PolyFloat,
        transpose: PolyFloat,
        quantize: i32,
    ) -> PolyFloat {
        if quantize == 0 {
            return input_midi + transpose;
        }

        let global_transpose = utils::is_transpose_quantize_global(quantize);
        let mut pre_add = PolyFloat::zero();
        let mut post_add = input_midi;
        if global_transpose {
            pre_add = input_midi;
            post_add = PolyFloat::zero();
        }

        let snapped = utils::snap_transpose(pre_add + transpose, quantize);

        if self.transpose_quantize != 0 {
            self.phase_inc *= utils::note_offset_to_ratio(snapped - self.last_quantized_transpose);
        }

        self.last_quantized_transpose = snapped;
        self.transpose_quantize = quantize;
        post_add + snapped
    }
}

impl Default for SampleSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorImpl for SampleSource {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let mut current_pan_amplitude = self.pan_amplitude;
        let input_pan = utils::clamp(this.input(PAN).at(0), -1.0, 1.0);
        self.pan_amplitude = futils::pan_amplitude(input_pan);

        let mut input_midi = PolyFloat::zero();
        if this.input(KEYTRACK).at(0).get(0) != 0.0 {
            input_midi = this.input(MIDI).at(0) - MIDI_TRACK_CENTER as f32;
        }

        let transpose_quantize = float_to_i32(this.input(TRANSPOSE_QUANTIZE).at(0).get(0));
        let mut transpose =
            self.snap_transpose(input_midi, this.input(TRANSPOSE).at(0), transpose_quantize);
        transpose = utils::clamp(
            transpose + this.input(TUNE).at(0),
            MIN_TRANSPOSE,
            MAX_TRANSPOSE,
        );

        let sample = self.sample.clone();
        let sample = sample.borrow();
        let sample_rate_ratio =
            (1.0 * sample.active_sample_rate() as f32) / this.sample_rate() as f32;
        let mut current_phase_inc = self.phase_inc;
        self.phase_inc = utils::cents_to_ratio(transpose * CENTS_PER_NOTE as f32)
            * sample_rate_ratio
            * (1 << Sample::UPSAMPLE_TIMES) as f32;

        let audio_length = sample.active_length() as i32;
        let audio_length_f = audio_length as f32;
        let reset_mask = this.get_reset_mask(RESET);
        let mut reset_offset = utils::to_float(this.input(RESET).trigger_offset());
        current_pan_amplitude =
            utils::mask_load(current_pan_amplitude, self.pan_amplitude, reset_mask);
        current_phase_inc = utils::mask_load(current_phase_inc, self.phase_inc, reset_mask);
        self.bounce_mask &= !reset_mask;
        reset_offset *= current_phase_inc;

        let mut reset_value = -reset_offset;
        if this.input(RANDOM_PHASE).at(0).get(0) != 0.0 {
            reset_value = PolyFloat::splat(self.random_generator.next() * audio_length_f);
            reset_value = utils::mask_load(
                reset_value,
                self.random_generator.next() * audio_length_f,
                constants::first_mask(),
            );
            reset_value -= reset_offset;
        }

        self.sample_index =
            utils::mask_load(self.sample_index, utils::floor(reset_value), reset_mask);
        self.sample_fraction = utils::mask_load(
            self.sample_fraction,
            reset_value - self.sample_index,
            reset_mask,
        );

        let looping = this.input(LOOP).at(0).get(0) != 0.0;
        let mut loop_enabled_mask = PolyMask::zero();
        if looping {
            loop_enabled_mask = constants::full_mask();
        }

        let bounce = this.input(BOUNCE).at(0).get(0) != 0.0;
        let mut bounce_enabled_mask = PolyMask::zero();
        if bounce {
            bounce_enabled_mask = constants::full_mask();
        } else {
            self.bounce_mask = PolyMask::zero();
        }

        let mut audio_buffers: [&[f32]; 4] = [&[]; 4];
        let mut phase_mult = PolyFloat::splat(1.0);
        for (i, buffer) in audio_buffers.iter_mut().enumerate() {
            let index = sample.active_index(self.phase_inc.get(i));
            *buffer = if looping && !bounce {
                if i % 2 != 0 {
                    sample.active_right_loop_buffer(index)
                } else {
                    sample.active_left_loop_buffer(index)
                }
            } else if i % 2 != 0 {
                sample.active_right_buffer(index)
            } else {
                sample.active_left_buffer(index)
            };

            phase_mult.set(i, 1.0 / (1 << index) as f32);
        }

        let sample_inc = 1.0 / num_samples as f32;
        let delta_pan_amplitude = (self.pan_amplitude - current_pan_amplitude) * sample_inc;
        let delta_phase_inc = (self.phase_inc - current_phase_inc) * sample_inc;

        let raw_output = this.output_buffer(RAW);
        let mut current_fraction = self.sample_fraction;
        let mut current_index = utils::min(self.sample_index, PolyFloat::splat(audio_length_f));

        let mut current_bounce = self.bounce_mask;
        for output in raw_output.iter().take(num_samples) {
            current_phase_inc += delta_phase_inc;

            let adjusted = utils::mask_load(
                current_index,
                PolyFloat::splat(audio_length_f) - current_index,
                current_bounce,
            );
            let index_phase = utils::max(adjusted, PolyFloat::zero()) * phase_mult;
            let fraction_phase = current_fraction * phase_mult;

            let start_indices = utils::floor_to_int(index_phase);
            let rounded_down_phase = utils::to_float(start_indices);
            let mut t = index_phase - rounded_down_phase + fraction_phase;

            t = utils::mask_load(t, PolyFloat::splat(1.0) - t, current_bounce);

            let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
            let mut value_matrix = utils::get_value_matrix_separate(audio_buffers, start_indices);
            value_matrix.transpose();
            output.set(interpolation_matrix.multiply_and_sum_rows(&value_matrix));

            current_fraction += current_phase_inc;
            let increment = utils::floor(current_fraction);
            current_fraction -= increment;

            current_index += increment;
            let done_mask =
                PolyFloat::greater_than_or_equal(current_index, PolyFloat::splat(audio_length_f));
            let bounced_mask = done_mask & !current_bounce & bounce_enabled_mask;
            let loop_over_mask =
                done_mask & (current_bounce | !bounce_enabled_mask) & loop_enabled_mask;
            current_bounce = (bounced_mask | current_bounce) & !loop_over_mask;

            current_index = utils::mask_load(
                current_index,
                current_index - audio_length_f,
                bounced_mask | loop_over_mask,
            );
            current_index = utils::min(PolyFloat::splat(audio_length_f), current_index);
            current_fraction &= !done_mask;
        }

        self.bounce_mask = current_bounce;
        if reset_mask.any_mask() != 0 {
            this.clear_output_buffer_for_reset(reset_mask, RESET, RAW);
        }

        let level_input = this.input_buffer(LEVEL);
        let levelled_output = this.output_buffer(LEVELLED);
        let zero = PolyFloat::zero();
        let max = PolyFloat::splat(MAX_AMPLITUDE);
        for i in 0..num_samples {
            current_pan_amplitude += delta_pan_amplitude;
            let level = utils::clamp(level_input[i].get(), zero, max);
            levelled_output[i].set(current_pan_amplitude * level * level * raw_output[i].get());
        }

        self.sample_index = current_index;
        self.sample_fraction = current_fraction;
        let mut phase = utils::mask_load(
            self.sample_index,
            PolyFloat::splat(audio_length_f) - self.sample_index,
            self.bounce_mask,
        );
        phase *= 1.0 / audio_length_f;
        self.phase_output.set_at(
            0,
            utils::encode_phase_and_voice(phase, this.input(NOTE_COUNT).at(0)),
        );
    }
}
