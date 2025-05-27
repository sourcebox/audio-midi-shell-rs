//! Simple monophonic synthesizer generating a sine wave for each received MIDI note.

use audio_midi_shell::{AudioGenerator, AudioMidiShell};

const SAMPLE_RATE: u32 = 44100;
const BLOCK_SIZE: usize = 16;

fn main() -> ! {
    AudioMidiShell::run_forever(SAMPLE_RATE, BLOCK_SIZE, SineSynth::default());
}

#[derive(Debug, Default)]
struct SineSynth {
    level: f32,
    phase: f32,
    phase_inc: f32,
}

impl AudioGenerator for SineSynth {
    fn init(&mut self, _block_size: usize) {}

    fn process(&mut self, samples_left: &mut [f32], samples_right: &mut [f32]) {
        for (sample_left, sample_right) in samples_left.iter_mut().zip(samples_right.iter_mut()) {
            let sample = f32::sin(self.phase) * self.level * 0.5;
            *sample_left = sample;
            *sample_right = sample;

            self.phase += self.phase_inc;

            if self.phase > core::f32::consts::TAU {
                self.phase -= core::f32::consts::TAU;
            }
        }
    }

    fn process_midi(&mut self, message: Vec<u8>) {
        match message[0] & 0xF0 {
            0x80 => {
                // Note off
                self.level = 0.0;
            }
            0x90 => {
                // Note on
                self.level = message[2] as f32 / 127.0;
                let frequency = 440.0 * f32::powf(2.0, (message[1] as i32 - 69) as f32 / 12.0);
                self.phase_inc = frequency / SAMPLE_RATE as f32 * core::f32::consts::TAU;
            }
            _ => {}
        };
    }
}
