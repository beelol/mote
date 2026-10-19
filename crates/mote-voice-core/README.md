# mote-voice-core

Portable deterministic voice decisions for Mote and other local consumers. The crate converts
bounded 16 kHz mono frames, detects speech, decides utterance endpoints and classifies barge-in
words. It contains no capture, speech recognition, model, daemon, operating-system or network
integration.

The algorithms and deterministic fixture expectations were derived from `voice/src/pcm.rs`,
`voice/src/gate.rs`, `voice/src/endpoint.rs`, `voice/src/words.rs` and `voice/src/synth.rs` in
`beelol/overseer` revision `2b1a19a9b6e639628367a22e16155af20df4f86a`. Names and vocabulary
were made product-neutral. Callers supply their own backchannels, stop phrases and recently spoken
words through `Vocabulary`.

The only non-standard-library dependency is `mote-voice-protocol`. The dependency boundary is
checked by an integration test.
