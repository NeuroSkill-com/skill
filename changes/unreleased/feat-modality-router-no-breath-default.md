### Features

- **Modality Router**: Added a decision table mapping 12 EEG triggers to 7 intervention modalities (Breath, Tactile, Cognitive, Visual, Movement, Auditory, Passive Physiological) so the LLM selects the best modality for each person's circumstances before choosing a specific protocol. Breathing is presented as one equal option among many, never the default. Includes a modality selection guide by context and phrasing examples showing how to offer multi-modal choices.

- **Multi-modal protocol restructuring**: Restructured ~30 protocols that previously defaulted to breathing to present non-breathing alternatives at equal priority using ", OR " choice points. Affected protocols include Focus Reset, Pre-Performance Activation, Extended Exhale, Physiological Sigh, Kapalabhati, 4-Count Energising, Wim Hof, Anger Processing, Grief Holding, Emotion Surfing, Joy Amplification, Emotional Boundaries, Excitement Regulation, all morning routines, all workout protocols, social media protocols, Pre-Meal Pause, Sleep Wind-Down, Co-Regulation, One-Handed Calm, Cortical Quieting, Coherence Building, Break Reset, and Post-Scroll Reset.

- **Matching Guidance updated**: Added "Modality first, protocol second" as the top matching rule and "Always offer at least two modalities" as a new requirement, ensuring the LLM never prescribes breathing without presenting alternatives.
