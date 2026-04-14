// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** KO — barrel export merging all namespace files. */

import calibration from "./calibration";
import common from "./common";
import dashboard from "./dashboard";
import help from "./help";
import helpRef from "./help-ref";
import history from "./history";
import hooks from "./hooks";
import llm from "./llm";
import onboarding from "./onboarding";
import perm from "./perm";
import screenshots from "./screenshots";
import search from "./search";
import settings from "./settings";
import tts from "./tts";
import ui from "./ui";
import virtualEeg from "./virtual-eeg";

const all: Record<string, string> = {
  ...common,
  ...dashboard,
  ...settings,
  ...search,
  ...calibration,
  ...history,
  ...hooks,
  ...llm,
  ...onboarding,
  ...screenshots,
  ...tts,
  ...perm,
  ...help,
  ...helpRef,
  ...ui,
  ...virtualEeg,
};

export default all;
