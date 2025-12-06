#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::{collections::VecDeque, sync::mpsc};

use interflow::prelude::*;
use midir::{MidiInput, MidiInputConnection};

/// Shell running the audio and MIDI processing.
pub struct AudioMidiShell {
    /// MIDI connections.
    pub midi_connections: MidiConnections,
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

        let device = default_output_device();

        #[cfg(target_os = "macos")]
        let channels = 0b11;

        #[cfg(not(target_os = "macos"))]
        let channels = 2;

        let stream_config = StreamConfig {
            samplerate: sample_rate as f64,
            channels,
            buffer_size_range: (Some(buffer_size), Some(buffer_size)),
            exclusive: false,
        };
        let output_stream = device
            .create_output_stream(
                stream_config,
                OutputCallback::new(generator, midi_receiver, process_chunk_size),
            )
            .unwrap();

        // TODO: store stream correctly when `interflow` API allows it.
        std::mem::forget(output_stream);

        Self { midi_connections }
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

/// Callback for the output stream.
struct OutputCallback<G: AudioGenerator> {
    /// Generator.
    generator: G,

    /// Receiver for MIDI messages.
    midi_receiver: mpsc::Receiver<(u64, Vec<u8>)>,

    /// Number of samples passed to the `process` function.
    process_chunk_size: usize,

    /// Samples to output.
    out_samples: VecDeque<(f32, f32)>,
}

impl<G: AudioGenerator> AudioOutputCallback for OutputCallback<G> {
    fn on_output_data(&mut self, _context: AudioCallbackContext, mut output: AudioOutput<f32>) {
        for i in 0..output.buffer.num_samples() {
            if self.out_samples.is_empty() {
                while let Ok(message) = self.midi_receiver.try_recv() {
                    self.generator.process_midi(message.1.as_ref(), message.0);
                }

                let mut frames = vec![[0.0; 2]; self.process_chunk_size];
                self.generator.process(&mut frames);

                for i in 0..self.process_chunk_size {
                    self.out_samples.push_back((frames[i][0], frames[i][1]));
                }
            }

            if let Some(s) = self.out_samples.pop_front() {
                output.buffer.set_frame(i, &[s.0, s.1]);
            }
        }
    }
}

impl<G: AudioGenerator> OutputCallback<G> {
    /// Returns a new callback.
    pub fn new(
        generator: G,
        midi_receiver: mpsc::Receiver<(u64, Vec<u8>)>,
        chunk_size: usize,
    ) -> Self {
        Self {
            generator,
            midi_receiver,
            process_chunk_size: chunk_size,
            out_samples: VecDeque::with_capacity(chunk_size),
        }
    }
}
