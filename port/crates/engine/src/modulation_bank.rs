// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ModulationConnection` and `vital::ModulationConnectionBank`
//! (`vital/src/common/synth_types.{h,cpp}`).

use sas_dsp_core::constants::MAX_MODULATION_CONNECTIONS;
use sas_dsp_core::ProcessorRef;
use sas_modulation::ModulationConnectionProcessor;
use sas_preset::engine::is_source_default_bipolar;

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

    /// `resetConnection(from, to)`.
    pub fn reset_connection(&mut self, from: &str, to: &str) {
        self.source_name = from.to_string();
        self.destination_name = to.to_string();
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
                source_name: String::new(),
                destination_name: String::new(),
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
            .position(|(i, connection)| {
                to != format!("modulation_{}_amount", i + 1) && connection.is_available()
            })?;
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
