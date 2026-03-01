// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * EEG electrode metadata and positions for the 10-20, 10-10, and 10-5 systems.
 *
 * 3D positions are derived from MNE-Python's standard montage files
 * (standard_1020.elc and standard_1005.elc) and stored as unit direction
 * vectors from the head center in electrode-positions.json.
 *
 * At runtime the 3D component raycasts each direction onto the actual head
 * mesh surface to place electrodes accurately.
 */

import positionsData from "./electrode-positions.json";

export type BrainRegion =
  | "prefrontal"
  | "frontal"
  | "central"
  | "temporal"
  | "parietal"
  | "occipital"
  | "reference";

export type ElectrodeSystem = "10-20" | "10-10" | "10-5";

export interface Electrode {
  name: string;
  /** Raw position in Three.js coords [x, y, z] in mm (from MNE standard montage) */
  pos: [number, number, number];
  region: BrainRegion;
  lobe: string;
  function: string;
  signals: string;
  systems: ElectrodeSystem[];
  muse?: boolean;
  museRole?: string;
}

export const regionColors: Record<BrainRegion, string> = {
  prefrontal: "#22c55e",
  frontal:    "#3b82f6",
  central:    "#a855f7",
  temporal:   "#f59e0b",
  parietal:   "#ec4899",
  occipital:  "#ef4444",
  reference:  "#6b7280",
};

export const regionLabels: Record<BrainRegion, string> = {
  prefrontal: "Prefrontal Cortex",
  frontal:    "Frontal Lobe",
  central:    "Central / Motor",
  temporal:   "Temporal Lobe",
  parietal:   "Parietal Lobe",
  occipital:  "Occipital Lobe",
  reference:  "Reference",
};

export const regionDescriptions: Record<BrainRegion, string> = {
  prefrontal: "Executive function, decision-making, personality, working memory, emotional regulation, social behavior, and attention control. Alpha suppression here indicates engagement and cognitive effort.",
  frontal:    "Motor planning, speech production (Broca's area on left), voluntary movement, reasoning, and problem-solving. Beta activity relates to active thinking; theta to creative flow and error monitoring.",
  central:    "Primary motor cortex and somatosensory cortex. Controls voluntary movement and processes touch, pressure, temperature. Mu rhythm (8-13 Hz) suppression indicates motor imagery or execution.",
  temporal:   "Auditory processing, language comprehension (Wernicke's area on left), memory encoding (hippocampus below), emotion (amygdala below), music, and face recognition. Strong theta during memory tasks.",
  parietal:   "Spatial awareness, sensory integration, attention orienting, mathematical processing, and body awareness. Alpha activity here relates to idle sensory processing; suppression indicates active spatial attention.",
  occipital:  "Visual processing — primary visual cortex (V1). Strong alpha (8-13 Hz) when eyes closed; suppression when eyes open or processing visual information. Visual evoked potentials originate here.",
  reference:  "Reference and ground electrodes used for signal comparison. Not directly measuring brain activity but essential for clean differential recordings.",
};

// ── Detailed metadata for notable electrodes ──
// Electrodes without an entry here get auto-generated descriptions.

interface ElectrodeMeta {
  region: BrainRegion;
  lobe: string;
  function: string;
  signals: string;
  muse?: boolean;
  museRole?: string;
}

const metadata: Record<string, ElectrodeMeta> = {
  // ─── Prefrontal ───
  Fpz:  { region: "prefrontal", lobe: "Prefrontal", function: "Frontopolar midline", signals: "Reference point between Fp1/Fp2. Measures bilateral prefrontal activity, eye blinks, and vertical EOG." },
  Fp1:  { region: "prefrontal", lobe: "Left prefrontal", function: "Left frontopolar", signals: "Left prefrontal cortex: executive function, approach motivation. Sensitive to eye blinks (EOG artifact)." },
  Fp2:  { region: "prefrontal", lobe: "Right prefrontal", function: "Right frontopolar", signals: "Right prefrontal cortex: withdrawal motivation, emotional regulation. Sensitive to eye blinks." },
  AF7:  { region: "prefrontal", lobe: "Left prefrontal", function: "Left anterior-frontal", signals: "Left prefrontal: executive function, approach motivation, positive affect, working memory. Picks up blink artifacts.", muse: true, museRole: "EEG Channel 2 — left frontal. Drives FAA (Frontal Alpha Asymmetry), Focus, Engagement, Cognitive Load." },
  AF3:  { region: "prefrontal", lobe: "Left prefrontal", function: "Left anterior-frontal (medial)", signals: "Left medial prefrontal: higher-level cognition, self-referential thought, emotional processing." },
  AFz:  { region: "prefrontal", lobe: "Medial prefrontal", function: "Anterior-frontal midline", signals: "Medial prefrontal: conflict monitoring, error detection (ERN), default mode network overlap." },
  AF4:  { region: "prefrontal", lobe: "Right prefrontal", function: "Right anterior-frontal (medial)", signals: "Right medial prefrontal: emotional regulation, vigilance, anxiety-related processing." },
  AF8:  { region: "prefrontal", lobe: "Right prefrontal", function: "Right anterior-frontal", signals: "Right prefrontal: withdrawal motivation, negative affect, emotional regulation, vigilance. Picks up blink artifacts.", muse: true, museRole: "EEG Channel 3 — right frontal. Drives FAA, Mood Index, Relaxation score." },

  // ─── Frontal ───
  F7:   { region: "frontal", lobe: "Left frontal", function: "Left inferior frontal", signals: "Left inferior frontal gyrus (near Broca's area): speech production, language processing, verbal working memory." },
  F5:   { region: "frontal", lobe: "Left frontal", function: "Left frontal (lateral)", signals: "Left lateral frontal: language-related motor planning, phonological processing." },
  F3:   { region: "frontal", lobe: "Left frontal", function: "Left frontal (mid)", signals: "Left mid-frontal: approach motivation, positive affect, verbal working memory. Key site for FAA studies." },
  F1:   { region: "frontal", lobe: "Left frontal", function: "Left frontal (medial)", signals: "Left medial frontal: higher-order motor planning, attention control." },
  Fz:   { region: "frontal", lobe: "Medial frontal", function: "Frontal midline", signals: "Frontal midline: theta (4-8 Hz) here = cognitive control, error monitoring, working memory load. Key site for frontal midline theta (FMθ)." },
  F2:   { region: "frontal", lobe: "Right frontal", function: "Right frontal (medial)", signals: "Right medial frontal: attention, cognitive control, inhibition." },
  F4:   { region: "frontal", lobe: "Right frontal", function: "Right frontal (mid)", signals: "Right mid-frontal: withdrawal motivation, emotional processing. FAA counterpart to F3." },
  F6:   { region: "frontal", lobe: "Right frontal", function: "Right frontal (lateral)", signals: "Right lateral frontal: spatial working memory, non-verbal cognitive tasks." },
  F8:   { region: "frontal", lobe: "Right frontal", function: "Right inferior frontal", signals: "Right inferior frontal: response inhibition (stop signal), emotional prosody, spatial attention." },
  FT7:  { region: "temporal", lobe: "Left frontotemporal", function: "Left frontotemporal", signals: "Left frontotemporal junction: semantic processing, language production support, auditory-motor integration." },
  FT8:  { region: "temporal", lobe: "Right frontotemporal", function: "Right frontotemporal", signals: "Right frontotemporal: prosody processing, emotional voice processing, non-verbal auditory processing." },
  FC5:  { region: "frontal", lobe: "Left frontocentral", function: "Left frontocentral (lateral)", signals: "Left lateral frontocentral: premotor cortex, motor preparation for right-hand movements." },
  FC3:  { region: "frontal", lobe: "Left frontocentral", function: "Left frontocentral", signals: "Left frontocentral: supplementary motor area, motor preparation. Mu rhythm site." },
  FC1:  { region: "frontal", lobe: "Left frontocentral", function: "Left frontocentral (medial)", signals: "Left medial frontocentral: fine motor control, bimanual coordination." },
  FCz:  { region: "frontal", lobe: "Medial frontocentral", function: "Frontocentral midline", signals: "Frontocentral midline: error-related negativity (ERN), motor monitoring, pre-movement potentials." },
  FC2:  { region: "frontal", lobe: "Right frontocentral", function: "Right frontocentral (medial)", signals: "Right medial frontocentral: motor control, action monitoring." },
  FC4:  { region: "frontal", lobe: "Right frontocentral", function: "Right frontocentral", signals: "Right frontocentral: left-hand motor preparation, premotor activity." },
  FC6:  { region: "frontal", lobe: "Right frontocentral", function: "Right frontocentral (lateral)", signals: "Right lateral frontocentral: premotor cortex, motor planning for left-side movements." },

  // ─── Central ───
  T7:   { region: "temporal", lobe: "Left temporal", function: "Left mid-temporal", signals: "Left mid-temporal: auditory cortex, language comprehension (Wernicke's area), verbal memory. Theta bursts during memory encoding." },
  C5:   { region: "central", lobe: "Left central", function: "Left central (lateral)", signals: "Left lateral central: lateral primary motor cortex, sensory processing for right body." },
  C3:   { region: "central", lobe: "Left central", function: "Left central", signals: "Left central: primary motor cortex for right hand. Mu rhythm (8-13 Hz) suppression = motor imagery/execution. Key BCI electrode." },
  C1:   { region: "central", lobe: "Left central", function: "Left central (medial)", signals: "Left medial central: foot and leg motor area, midline sensorimotor processing." },
  Cz:   { region: "central", lobe: "Vertex", function: "Central midline (vertex)", signals: "Vertex: topmost point of the skull. Midline motor and sensory. Auditory P300 and vertex sharp waves in sleep." },
  C2:   { region: "central", lobe: "Right central", function: "Right central (medial)", signals: "Right medial central: foot and leg motor area for left side." },
  C4:   { region: "central", lobe: "Right central", function: "Right central", signals: "Right central: primary motor cortex for left hand. Mu rhythm suppression for left-hand motor imagery." },
  C6:   { region: "central", lobe: "Right central", function: "Right central (lateral)", signals: "Right lateral central: lateral motor cortex, sensory processing for left body." },
  T8:   { region: "temporal", lobe: "Right temporal", function: "Right mid-temporal", signals: "Right mid-temporal: music processing, prosody, spatial hearing, non-verbal auditory processing, face recognition." },

  // ─── Temporal-Parietal ───
  TP9:  { region: "temporal", lobe: "Left temporal", function: "Left temporoparietal (mastoid)", signals: "Left mastoid: behind the left ear. Left temporal lobe activity, auditory processing, language. Sensitive to muscle EMG artefacts.", muse: true, museRole: "EEG Channel 1 — left ear sensor. Provides left reference for Laterality Index and Alpha Coherence." },
  TP7:  { region: "temporal", lobe: "Left temporoparietal", function: "Left temporoparietal", signals: "Left temporoparietal junction: language comprehension, social cognition (theory of mind), multisensory integration." },
  CP5:  { region: "parietal", lobe: "Left centroparietal", function: "Left centroparietal (lateral)", signals: "Left lateral centroparietal: somatosensory association cortex, sensory integration." },
  CP3:  { region: "parietal", lobe: "Left centroparietal", function: "Left centroparietal", signals: "Left centroparietal: somatosensory cortex for right body, tactile discrimination." },
  CP1:  { region: "parietal", lobe: "Left centroparietal", function: "Left centroparietal (medial)", signals: "Left medial centroparietal: midline somatosensory processing." },
  CPz:  { region: "parietal", lobe: "Centroparietal midline", function: "Centroparietal midline", signals: "Centroparietal midline: P300 component in oddball paradigms, sensorimotor integration." },
  CP2:  { region: "parietal", lobe: "Right centroparietal", function: "Right centroparietal (medial)", signals: "Right medial centroparietal: midline sensorimotor processing." },
  CP4:  { region: "parietal", lobe: "Right centroparietal", function: "Right centroparietal", signals: "Right centroparietal: somatosensory cortex for left body." },
  CP6:  { region: "parietal", lobe: "Right centroparietal", function: "Right centroparietal (lateral)", signals: "Right lateral centroparietal: somatosensory association, spatial processing." },
  TP8:  { region: "temporal", lobe: "Right temporoparietal", function: "Right temporoparietal", signals: "Right temporoparietal junction: spatial attention, theory of mind, vestibular processing." },
  TP10: { region: "temporal", lobe: "Right temporal", function: "Right temporoparietal (mastoid)", signals: "Right mastoid: behind the right ear. Right temporal lobe, prosody, music, spatial awareness. Sensitive to muscle EMG artefacts.", muse: true, museRole: "EEG Channel 4 — right ear sensor. Provides right reference for bilateral temporal coverage and coherence." },

  // ─── Parietal ───
  P7:   { region: "parietal", lobe: "Left parietal", function: "Left inferior parietal", signals: "Left inferior parietal: reading, calculation, left angular gyrus. Semantic memory and tool use." },
  P5:   { region: "parietal", lobe: "Left parietal", function: "Left parietal (lateral)", signals: "Left lateral parietal: language-related spatial processing, numerical cognition." },
  P3:   { region: "parietal", lobe: "Left parietal", function: "Left parietal", signals: "Left parietal: P300 component, spatial attention to right visual field, arithmetic. Key for P300 BCI." },
  P1:   { region: "parietal", lobe: "Left parietal", function: "Left parietal (medial)", signals: "Left medial parietal: precuneus overlap, self-referential processing, spatial orientation." },
  Pz:   { region: "parietal", lobe: "Parietal midline", function: "Parietal midline", signals: "Parietal midline: largest P300 amplitude in oddball tasks. Alpha dominant when resting with eyes open. Key for SSVEP BCI." },
  P2:   { region: "parietal", lobe: "Right parietal", function: "Right parietal (medial)", signals: "Right medial parietal: precuneus, visuo-spatial processing." },
  P4:   { region: "parietal", lobe: "Right parietal", function: "Right parietal", signals: "Right parietal: spatial attention to left visual field, spatial reasoning, mental rotation." },
  P6:   { region: "parietal", lobe: "Right parietal", function: "Right parietal (lateral)", signals: "Right lateral parietal: spatial attention, navigation, body schema." },
  P8:   { region: "parietal", lobe: "Right parietal", function: "Right inferior parietal", signals: "Right inferior parietal: spatial awareness, attention orienting, face processing." },

  // ─── Occipital ───
  PO7:  { region: "occipital", lobe: "Left parieto-occipital", function: "Left parieto-occipital", signals: "Left parieto-occipital: visual word form area, reading-related processing." },
  PO3:  { region: "occipital", lobe: "Left parieto-occipital", function: "Left parieto-occipital (medial)", signals: "Left medial parieto-occipital: higher-order visual processing, visual attention." },
  POz:  { region: "occipital", lobe: "Parieto-occipital midline", function: "Parieto-occipital midline", signals: "Parieto-occipital midline: strong alpha oscillations, SSVEP responses, visual attention." },
  PO4:  { region: "occipital", lobe: "Right parieto-occipital", function: "Right parieto-occipital (medial)", signals: "Right medial parieto-occipital: spatial visual processing, object recognition." },
  PO8:  { region: "occipital", lobe: "Right parieto-occipital", function: "Right parieto-occipital", signals: "Right parieto-occipital: face processing (N170 component), spatial visual attention." },
  O1:   { region: "occipital", lobe: "Left occipital", function: "Left occipital", signals: "Left occipital: primary visual cortex (V1) for right visual field. Strong alpha eyes-closed. Visual evoked potentials." },
  Oz:   { region: "occipital", lobe: "Occipital midline", function: "Occipital midline", signals: "Occipital midline: central visual cortex. Strongest alpha rhythm site. SSVEP target for BCI. Visual P100 component." },
  O2:   { region: "occipital", lobe: "Right occipital", function: "Right occipital", signals: "Right occipital: primary visual cortex for left visual field. Alpha reactivity and visual processing." },

  // ─── Reference ───
  LPA:  { region: "reference", lobe: "Left ear", function: "Left preauricular point", signals: "Left anatomical landmark. Used as reference point for coordinate systems." },
  RPA:  { region: "reference", lobe: "Right ear", function: "Right preauricular point", signals: "Right anatomical landmark. Used as reference point for coordinate systems." },
  Nz:   { region: "reference", lobe: "Nasion", function: "Nasion", signals: "Bridge of the nose. Anatomical landmark for electrode placement measurements." },
  A1:   { region: "reference", lobe: "Left earlobe", function: "Left earlobe reference", signals: "Left earlobe/mastoid reference. Common reference electrode for unipolar montages." },
  A2:   { region: "reference", lobe: "Right earlobe", function: "Right earlobe reference", signals: "Right earlobe/mastoid reference. Linked-ears reference uses average of A1+A2." },
  Iz:   { region: "occipital", lobe: "Inion", function: "Inion", signals: "Inion: occipital protuberance at the back of the skull. Landmark for 10-20 measurements." },
};

// ── Auto-classify electrodes by name prefix ──
function classifyByName(name: string): { region: BrainRegion; lobe: string } {
  const n = name.toUpperCase();
  if (n.startsWith("FP") || n.startsWith("AF")) return { region: "prefrontal", lobe: "Prefrontal" };
  if (n.startsWith("FT")) return { region: "temporal", lobe: "Frontotemporal" };
  if (n.startsWith("FC") || n.startsWith("F")) return { region: "frontal", lobe: "Frontal" };
  if (n.startsWith("TP")) return { region: "temporal", lobe: "Temporoparietal" };
  if (n.startsWith("T")) return { region: "temporal", lobe: "Temporal" };
  if (n.startsWith("CP")) return { region: "parietal", lobe: "Centroparietal" };
  if (n.startsWith("C")) return { region: "central", lobe: "Central" };
  if (n.startsWith("PO")) return { region: "occipital", lobe: "Parieto-occipital" };
  if (n.startsWith("P")) return { region: "parietal", lobe: "Parietal" };
  if (n.startsWith("O") || n.startsWith("I")) return { region: "occipital", lobe: "Occipital" };
  if (n === "NZ" || n.startsWith("M") || n.startsWith("A") || n === "LPA" || n === "RPA") return { region: "reference", lobe: "Reference" };
  return { region: "central", lobe: "Unknown" };
}

// ── Build the full electrode array from positions JSON + metadata ──
const posData = positionsData as unknown as Record<string, { pos: [number, number, number]; systems: ElectrodeSystem[] }>;

export const electrodes: Electrode[] = Object.entries(posData).map(([name, data]) => {
  const meta = metadata[name];
  const auto = classifyByName(name);
  return {
    name,
    pos: data.pos,
    region: meta?.region ?? auto.region,
    lobe: meta?.lobe ?? auto.lobe,
    function: meta?.function ?? `${auto.lobe} (${name})`,
    signals: meta?.signals ?? `${auto.lobe} electrode — ${name} position in the ${auto.region} region.`,
    systems: data.systems,
    muse: meta?.muse,
    museRole: meta?.museRole,
  };
});

export function getElectrodes(system: ElectrodeSystem): Electrode[] {
  return electrodes.filter(e => e.systems.includes(system));
}
