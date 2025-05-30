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
    /// - `buffer_size` is the number of samples used by the system buffer.
    ///   This setting determines the latency.
    /// - `chunk_size` is the number of samples passed to the `process` function.
    pub fn spawn(
        sample_rate: u32,
        buffer_size: usize,
        chunk_size: usize,
        mut generator: impl AudioGenerator + Send + 'static,
    ) -> Self {
        let (midi_sender, midi_receiver): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) =
            mpsc::channel();
        let midi_connections = init_midi(midi_sender);

        generator.init(chunk_size);

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
                OutputCallback::new(generator, midi_receiver, chunk_size),
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
    /// - `chunk_size` is the number of samples passed to the `process` function.
    pub fn run_forever(
        sample_rate: u32,
        buffer_size: usize,
        chunk_size: usize,
        generator: impl AudioGenerator + Send + 'static,
    ) -> ! {
        let _shell = Self::spawn(sample_rate, buffer_size, chunk_size, generator);

        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// Trait to be implemented by structs that are passed as generator to the shell.
pub trait AudioGenerator {
    /// Initializes the generator. Called once inside the shell `run` function.
    fn init(&mut self, _chunk_size: usize) {}

    /// Generates a chunk of samples.
    /// `samples_left` and `samples_right` are buffers of `chunk size` passed to the shell `run`
    /// function. They are initialized to `0.0` and must be filled with sample data.
    fn process(&mut self, samples_left: &mut [f32], samples_right: &mut [f32]);

    /// Processes a MIDI message.
    fn process_midi(&mut self, _message: Vec<u8>) {}
}

/// Vector of MIDI connections with an attached mpsc sender.
type MidiConnections = Vec<MidiInputConnection<mpsc::Sender<Vec<u8>>>>;

/// Connects all available MIDI inputs to an mpsc sender and returns them in a vector.
fn init_midi(sender: mpsc::Sender<Vec<u8>>) -> MidiConnections {
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
                |_timestamp, message, sender| {
                    sender.send(Vec::from(message)).ok();
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
    midi_receiver: mpsc::Receiver<Vec<u8>>,

    /// Number of samples passed to the `process` function.
    chunk_size: usize,

    /// Samples to output.
    out_samples: VecDeque<(f32, f32)>,
}

impl<G: AudioGenerator> AudioOutputCallback for OutputCallback<G> {
    fn on_output_data(&mut self, _context: AudioCallbackContext, mut output: AudioOutput<f32>) {
        for i in 0..output.buffer.num_samples() {
            if self.out_samples.is_empty() {
                while let Ok(message) = self.midi_receiver.try_recv() {
                    self.generator.process_midi(message);
                }

                let mut samples_left = vec![0.0; self.chunk_size];
                let mut samples_right = vec![0.0; self.chunk_size];
                self.generator
                    .process(&mut samples_left, &mut samples_right);
                for i in 0..self.chunk_size {
                    self.out_samples
                        .push_back((samples_left[i], samples_right[i]));
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
    pub fn new(generator: G, midi_receiver: mpsc::Receiver<Vec<u8>>, chunk_size: usize) -> Self {
        Self {
            generator,
            midi_receiver,
            chunk_size,
            out_samples: VecDeque::with_capacity(chunk_size),
        }
    }
}
