export type SupportedCompanyId = "muse" | "neurable" | "openbci" | "emotiv" | "idun" | "reak";

export interface SupportedDeviceItem {
  nameKey: string;
  image: string;
}

export interface SupportedCompany {
  id: SupportedCompanyId;
  nameKey: string;
  devices: SupportedDeviceItem[];
  instructionKeys: string[];
}

export const SUPPORTED_COMPANIES: SupportedCompany[] = [
  {
    id: "muse",
    nameKey: "settings.supportedDevices.company.muse",
    devices: [
      { nameKey: "settings.supportedDevices.device.muse2016", image: "/devices/muse-gen1.jpg" },
      { nameKey: "settings.supportedDevices.device.muse2", image: "/devices/muse-gen2.jpg" },
      { nameKey: "settings.supportedDevices.device.museS", image: "/devices/muse-s-gen1.jpg" },
      { nameKey: "settings.supportedDevices.device.museSAthena", image: "/devices/muse-s-athena.jpg" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.muse1",
      "settings.supportedDevices.instruction.muse2",
    ],
  },
  {
    id: "neurable",
    nameKey: "settings.supportedDevices.company.neurable",
    devices: [
      { nameKey: "settings.supportedDevices.device.mw75Neuro", image: "/devices/muse-mw75.jpg" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.neurable1",
      "settings.supportedDevices.instruction.neurable2",
    ],
  },
  {
    id: "openbci",
    nameKey: "settings.supportedDevices.company.openbci",
    devices: [
      { nameKey: "settings.supportedDevices.device.ganglion", image: "/devices/openbci-ganglion.jpg" },
      { nameKey: "settings.supportedDevices.device.cyton", image: "/devices/openbci-cyton.png" },
      { nameKey: "settings.supportedDevices.device.cytonDaisy", image: "/devices/openbci-cyton-daisy.jpg" },
      { nameKey: "settings.supportedDevices.device.galea", image: "/devices/openbci-galea.jpg" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.openbci1",
      "settings.supportedDevices.instruction.openbci2",
    ],
  },
  {
    id: "emotiv",
    nameKey: "settings.supportedDevices.company.emotiv",
    devices: [
      { nameKey: "settings.supportedDevices.device.epocX", image: "/devices/emotiv-epoc-x.webp" },
      { nameKey: "settings.supportedDevices.device.insight", image: "/devices/emotiv-insight.webp" },
      { nameKey: "settings.supportedDevices.device.flexSaline", image: "/devices/emotiv-flex-saline.webp" },
      { nameKey: "settings.supportedDevices.device.mn8", image: "/devices/emotiv-mn8.webp" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.emotiv1",
      "settings.supportedDevices.instruction.emotiv2",
    ],
  },
  {
    id: "idun",
    nameKey: "settings.supportedDevices.company.idun",
    devices: [
      { nameKey: "settings.supportedDevices.device.guardian", image: "/devices/idun-guardian.png" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.idun1",
      "settings.supportedDevices.instruction.idun2",
    ],
  },
  {
    id: "reak",
    nameKey: "settings.supportedDevices.company.reak",
    devices: [
      { nameKey: "settings.supportedDevices.device.nucleusHermes", image: "/devices/re-ak-nucleus-hermes.png" },
    ],
    instructionKeys: [
      "settings.supportedDevices.instruction.reak1",
      "settings.supportedDevices.instruction.reak2",
    ],
  },
];