### Dependencies

- **rlsl** and **rlsl-iroh** → 0.1.2. The two LSL fixes above are now also made upstream: rlsl's own `time_correction()` matches each answer to the query it echoes, and a lost stream is looked for by source id and name. DNS-SD discovery (rlsl's new `mdns` feature) is not enabled.
