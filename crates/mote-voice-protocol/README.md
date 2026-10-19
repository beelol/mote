# Shared voice protocol

This crate defines the local JSON Lines boundary between a voice listener and an agent runtime.
It contains no microphone, speech model, network or product runtime code. Production decoding
refuses commands that inject simulated audio or recognition results.

The wire shapes were derived from the voice protocol at Overseer revision
`2b1a19a9b6e639628367a22e16155af20df4f86a`, then made product-neutral. See `NOTICE`.
