# audio-midi-shell

Cross-platform wrapper around [interflow](https://github.com/SolarLiner/interflow/) and [midir](https://crates.io/crates/midir) for prototyping audio algorithms as standalone applications.

It opens the default output device with a given sample rate and process chunk size as well as all MIDI input ports found.

## Usage

```rust no_run
use audio_midi_shell::{AudioMidiShell, AudioGenerator};

const SAMPLE_RATE: u32 = 44100;
const BUFFER_SIZE: usize = 1024;
const CHUNK_SIZE: usize = 16;

fn main() -> ! {
    AudioMidiShell::run_forever(SAMPLE_RATE, BUFFER_SIZE, CHUNK_SIZE, TestGenerator);
}

struct TestGenerator;

impl AudioGenerator for TestGenerator {
    fn init(&mut self, chunk_size: usize) {
        // Optional function, called once on startup for initialization tasks.
    }

    fn process(&mut self, samples_left: &mut [f32], samples_right: &mut [f32]) {
        // Called periodically with buffers of `CHUNK_SIZE` samples.
        // Fill `samples_left` and `samples_right` with audio data accordingly.
    }

    fn process_midi(&mut self, message: Vec<u8>) {
        // Optional function, called on each incoming MIDI message.
    }
}

```

## Example

The `examples` directory contains a simple monophonic synthesizer playing a sine wave for each received note.

```shell
    cargo run --example sine_synth
```

## License

Published under the MIT license. Any contribution to this project must be provided under the same license conditions.

Author: Oliver Rockstedt <info@sourcebox.de>
