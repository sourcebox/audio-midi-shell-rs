#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::{collections::VecDeque, sync::mpsc};

use midir::{MidiInput, MidiInputConnection};
use tinyaudio::{OutputDevice, OutputDeviceParameters, run_output_device};

/// Shell running the audio and MIDI processing.
pub struct AudioMidiShell {
    /// MIDI connections.
    pub midi_connections: MidiConnections,

    /// Output device:
    pub output_device: OutputDevice,
}

impl AudioMidiShell {
    /// Initializes the MIDI inputs, the output device and runs the generator in a callback.
    /// It returns a shell object that must be kept alive.
    /// - `sample_rate` is the sampling frequency in Hz.
    /// - `buffer_size` is the number of frames used by the system buffer.
    ///   This setting determines the latency.
    /// - `process_chunk_size` is the number of frames passed to the `process` function.
    pub fn spawn(
        sample_rate: u32,
        buffer_size: usize,
        process_chunk_size: usize,
        mut generator: impl AudioGenerator + Send + 'static,
    ) -> Self {
        let (midi_sender, midi_receiver) = mpsc::channel();
        let midi_connections = init_midi(midi_sender);

        generator.init(process_chunk_size);

        let params = OutputDeviceParameters {
            channels_count: 2,
            sample_rate: sample_rate as usize,
            channel_sample_count: buffer_size,
        };

        let mut out_samples = VecDeque::with_capacity(process_chunk_size);

        let output_device = run_output_device(params, move |data| {
            for samples in data.chunks_mut(params.channels_count) {
                if out_samples.is_empty() {
                    while let Ok(message) = midi_receiver.try_recv() {
                        generator.process_midi(message.1.as_ref(), message.0);
                    }

                    let mut frames = vec![[0.0; 2]; process_chunk_size];
                    generator.process(&mut frames);

                    for frame in frames.iter().take(process_chunk_size) {
                        out_samples.push_back(*frame);
                    }
                }

                if let Some(s) = out_samples.pop_front() {
                    samples[0] = s[0];
                    samples[1] = s[1];
                }
            }
        })
        .unwrap();

        Self {
            midi_connections,
            output_device,
        }
    }

    /// Spawns the shell and keeps it alive forever.
    /// - `sample_rate` is the sampling frequency in Hz.
    /// - `buffer_size` is the number of samples used by the system buffer.
    ///   This setting determines the latency.
    /// - `process_chunk_size` is the number of samples passed to the `process` function.
    pub fn run_forever(
        sample_rate: u32,
        buffer_size: usize,
        process_chunk_size: usize,
        generator: impl AudioGenerator + Send + 'static,
    ) -> ! {
        let _shell = Self::spawn(sample_rate, buffer_size, process_chunk_size, generator);

        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// Trait to be implemented by structs that are passed as generator to the shell.
pub trait AudioGenerator {
    /// Initializes the generator. Called once on invocation.
    /// - `process_chunk_size` is the number of frames passed to the `process` function.
    fn init(&mut self, _process_chunk_size: usize) {}

    /// Generates a chunk of samples.
    /// - `frames` is a buffer of `process_chunk_size` elements.
    ///   It is initialized to `[0.0; 2]` and must be filled with sample data.
    ///   Index `0` of each element is the left channel, index `1` the right channel.
    fn process(&mut self, frames: &mut [[f32; 2]]);

    /// Processes a MIDI message.
    fn process_midi(&mut self, _message: &[u8], _timestamp: u64) {}
}

/// Vector of MIDI connections with an attached mpsc sender.
type MidiConnections = Vec<MidiInputConnection<mpsc::Sender<(u64, Vec<u8>)>>>;

/// Connects all available MIDI inputs to an mpsc sender and returns them in a vector.
fn init_midi(sender: mpsc::Sender<(u64, Vec<u8>)>) -> MidiConnections {
    let mut connections = MidiConnections::new();

    let input = MidiInput::new(&(env!("CARGO_PKG_NAME").to_owned() + " scan input"))
        .expect("MIDI Input error");

    for port in input.ports().iter() {
        let input = MidiInput::new(&(env!("CARGO_PKG_NAME").to_owned() + " input"))
            .expect("MIDI Input error");
        let port_name = input.port_name(port).unwrap();
        log::info!("Connecting to MIDI input {}", port_name);
        let conn = input
            .connect(
                port,
                port_name.as_str(),
                |timestamp, message, sender| {
                    sender.send((timestamp, Vec::from(message))).ok();
                },
                sender.clone(),
            )
            .ok();
        connections.push(conn.unwrap());
    }

    connections
}
