#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

use std::sync::mpsc;

use midir::{MidiInput, MidiInputConnection};
use tinyaudio::{run_output_device, BaseAudioOutputDevice, OutputDeviceParameters};

/// Shell running the audio and MIDI processing.
pub struct AudioMidiShell {
    /// MIDI connections.
    pub midi_connections: MidiConnections,

    /// Output device:
    pub output_device: Box<dyn BaseAudioOutputDevice>,
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

        let params = OutputDeviceParameters {
            channels_count: 2,
            sample_rate: sample_rate as usize,
            channel_sample_count: block_size,
        };

        let output_device = run_output_device(params, move |data| {
            while let Ok(message) = midi_receiver.try_recv() {
                generator.process_midi(message);
            }

            let mut samples_left = vec![0.0; block_size];
            let mut samples_right = vec![0.0; block_size];
            generator.process(&mut samples_left, &mut samples_right);

            for (frame_no, samples) in data.chunks_mut(params.channels_count).enumerate() {
                samples[0] = samples_left[frame_no];
                samples[1] = samples_right[frame_no];
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
