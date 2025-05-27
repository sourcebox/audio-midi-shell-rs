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
    /// - `block_size` is the number of samples for the `process` function.
    pub fn spawn(
        sample_rate: u32,
        block_size: usize,
        mut generator: impl AudioGenerator + Send + 'static,
    ) -> Self {
        let (midi_sender, midi_receiver): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) =
            mpsc::channel();
        let midi_connections = init_midi(midi_sender);

        generator.init(block_size);

        let device = default_output_device();
        let stream_config = StreamConfig {
            samplerate: sample_rate as f64,
            channels: 1,
            buffer_size_range: (None, None),
            exclusive: false,
        };
        let output_stream = device
            .create_output_stream(
                stream_config,
                OutputCallback::new(generator, midi_receiver, block_size),
            )
            .unwrap();

        // TODO: store stream correctly when `interflow` API allows it.
        std::mem::forget(output_stream);

        Self { midi_connections }
    }

    /// Spawns the shell and keeps it alive forever.
    /// - `sample_rate` is the sampling frequency in Hz.
    /// - `block_size` is the number of samples for the `process` function.
    pub fn run_forever(
        sample_rate: u32,
        block_size: usize,
        generator: impl AudioGenerator + Send + 'static,
    ) -> ! {
        let _shell = Self::spawn(sample_rate, block_size, generator);

        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// Trait to be implemented by structs that are passed as generator to the shell.
pub trait AudioGenerator {
    /// Initializes the generator. Called once inside the shell `run` function.
    fn init(&mut self, _block_size: usize) {}

    /// Generates a block of samples.
    /// `samples_left` and `samples_right` are buffers of the block size passed to the shell `run`
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

    /// Requested block size.
    block_size: usize,

    /// Samples to output.
    out_samples: VecDeque<(f32, f32)>,
}

impl<G: AudioGenerator> AudioOutputCallback for OutputCallback<G> {
    fn on_output_data(&mut self, _context: AudioCallbackContext, mut output: AudioOutput<f32>) {
        while let Ok(message) = self.midi_receiver.try_recv() {
            self.generator.process_midi(message);
        }

        for out in output.buffer.as_interleaved_mut() {
            if self.out_samples.is_empty() {
                let mut samples_left = vec![0.0; self.block_size];
                let mut samples_right = vec![0.0; self.block_size];
                self.generator
                    .process(&mut samples_left, &mut samples_right);
                for i in 0..self.block_size {
                    self.out_samples
                        .push_back((samples_left[i], samples_right[i]));
                }
            }

            if let Some(s) = self.out_samples.pop_front() {
                *out = s.0;
            }
        }
    }
}

impl<G: AudioGenerator> OutputCallback<G> {
    /// Returns a new callback.
    pub fn new(generator: G, midi_receiver: mpsc::Receiver<Vec<u8>>, block_size: usize) -> Self {
        Self {
            generator,
            midi_receiver,
            block_size,
            out_samples: VecDeque::with_capacity(block_size),
        }
    }
}
