# fwi

`fwi` is a pure Rust implementation of the six components of the 1987 Canadian Forest Fire Weather Index System: FFMC, DMC, DC, ISI, BUI, and FWI.

The crate performs no I/O and has no runtime or platform dependencies. It accepts noon local-standard-time weather observations, advances the three persistent moisture codes, and returns typed errors for invalid inputs.

The implementation is validated against the 48-day standard sequence distributed by the Natural Resources Canada `cffdrs` reference package. See the crate-level Rust documentation for the API, units, and references.

