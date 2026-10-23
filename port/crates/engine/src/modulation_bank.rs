// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ModulationConnection` and `vital::ModulationConnectionBank`
//! (`vital/src/common/synth_types.{h,cpp}`).

use sas_dsp_core::constants::MAX_MODULATION_CONNECTIONS;
use sas_dsp_core::ProcessorRef;
use sas_modulation::ModulationConnectionProcessor;
use sas_preset::engine::is_source_default_bipolar;

/// Bytes reserved for each slot's source and destination name: more than
/// the longest modulation source or destination in the registry.
const NAME_CAPACITY: usize = 64;

/// Whether `destination` is `"modulation_{index + 1}_amount"`, the amount of
/// slot `index` itself (compared without formatting the name).
fn is_own_amount(index: usize, destination: &str) -> bool {
    let Some(number) = destination
        .strip_prefix("modulation_")
        .and_then(|rest| rest.strip_suffix("_amount"))
    else {
        return false;
    };
    !number.is_empty()
        && !number.starts_with('0')
        && number.bytes().all(|b| b.is_ascii_digit())
        && number.parse::<usize>() == Ok(index + 1)
}

/// `ModulationConnection`: one slot of the bank and its processor.
pub struct ModulationConnection {
    pub source_name: String,
    pub destination_name: String,
    pub processor: ProcessorRef,
}

impl ModulationConnection {
    fn is_available(&self) -> bool {
        self.source_name.is_empty() && self.destination_name.is_empty()
    }

    /// `resetConnection(from, to)`. The names are copied into the slot's
    /// own strings, which hold any registry name without growing, so a
    /// connection made on the audio thread allocates nothing.
    pub fn reset_connection(&mut self, from: &str, to: &str) {
        self.source_name.clear();
        self.source_name.push_str(from);
        self.destination_name.clear();
        self.destination_name.push_str(to);
    }

    /// Whether the slot connects `source` to `destination`.
    pub fn connects(&self, source: &str, destination: &str) -> bool {
        self.source_name == source && self.destination_name == destination
    }
}

/// `ModulationConnectionBank`: `kMaxModulationConnections` slots, each with
/// its `ModulationConnectionProcessor(index)`.
pub struct ModulationConnectionBank {
    connections: Vec<ModulationConnection>,
}

impl Default for ModulationConnectionBank {
    fn default() -> Self {
        Self::new()
    }
}

impl ModulationConnectionBank {
    pub fn new() -> Self {
        let connections = (0..MAX_MODULATION_CONNECTIONS)
            .map(|i| ModulationConnection {
                source_name: String::with_capacity(NAME_CAPACITY),
                destination_name: String::with_capacity(NAME_CAPACITY),
                processor: ModulationConnectionProcessor::create(i as i32),
            })
            .collect();
        Self { connections }
    }

    /// `createConnection(from, to)`: the first free slot whose own amount is
    /// not the destination, with the source's default bipolar setting.
    pub fn create_connection(&mut self, from: &str, to: &str) -> Option<usize> {
        let index = self
            .connections
            .iter()
            .enumerate()
            .position(|(i, connection)| !is_own_amount(i, to) && connection.is_available())?;
        let connection = &mut self.connections[index];
        connection.reset_connection(from, to);
        let bipolar = is_source_default_bipolar(from);
        connection
            .processor
            .with::<ModulationConnectionProcessor, _>(|kernel, _| kernel.set_bipolar(bipolar));
        Some(index)
    }

    pub fn at_index(&self, index: usize) -> &ModulationConnection {
        &self.connections[index]
    }

    pub fn at_index_mut(&mut self, index: usize) -> &mut ModulationConnection {
        &mut self.connections[index]
    }

    pub fn num_connections(&self) -> usize {
        self.connections.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_amount_matches_the_formatted_name_only() {
        for i in 0..MAX_MODULATION_CONNECTIONS {
            let own = format!("modulation_{}_amount", i + 1);
            assert!(is_own_amount(i, &own));
            for other in 0..MAX_MODULATION_CONNECTIONS {
                assert_eq!(is_own_amount(other, &own), other == i);
            }
        }
        for name in [
            "modulation_01_amount",
            "modulation_+1_amount",
            "modulation__amount",
            "filter_1_cutoff",
        ] {
            assert!(!is_own_amount(0, name), "{name}");
        }
    }

    #[test]
    fn slot_names_fit_every_source_and_destination() {
        for name in sas_preset::engine::modulation_sources() {
            assert!(name.len() <= NAME_CAPACITY, "{name}");
        }
        for name in sas_preset::engine::modulation_destinations() {
            assert!(name.len() <= NAME_CAPACITY, "{name}");
        }
    }
}
