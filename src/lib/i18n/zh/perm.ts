// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** ZH "perm" namespace translations. */
const perm: Record<string, string> = {
  "perm.intro": "{app} 使用少量可选的操作系统权限来启用键盘/鼠标活动时间戳和通知等功能。所有数据均保留在您的设备上。",
  "perm.granted": "已授权",
  "perm.denied": "未授权",
  "perm.unknown": "未知",
  "perm.notRequired": "无需授权",
  "perm.systemManaged": "由操作系统管理",
  "perm.accessibility": "辅助功能",
  "perm.accessibilityDesc":
    "键盘和鼠标活动跟踪使用 CGEventTap（macOS）来记录最近一次按键和鼠标事件的时间戳。不会存储任何击键内容或光标位置——仅记录 Unix 秒级时间戳。此功能在 macOS 上需要辅助功能权限。",
  "perm.accessibilityOk": "权限已授予。正在记录键盘和鼠标活动时间戳。",
  "perm.accessibilityPending": "正在检查权限状态…",
  "perm.howToGrant": "如何授予此权限：",
  "perm.accessStep1": `点击下方的"打开辅助功能设置"。`,
  "perm.accessStep2": "在列表中找到 {app}（或点击 + 按钮添加）。",
  "perm.accessStep3": "将其开关打开。",
  "perm.accessStep4": "返回此页面——状态将自动更新。",
  "perm.openAccessibilitySettings": "打开辅助功能设置",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth 用于连接您的脑机接口头戴设备（Muse、MW75 Neuro、OpenBCI Ganglion、IDUN Guardian 等）。在 macOS 上，应用首次扫描时系统会提示授予 Bluetooth 访问权限。在 Linux 和 Windows 上无需单独授权。",
  "perm.openBluetoothSettings": "打开 Bluetooth 设置",
  "perm.notifications": "通知",
  "perm.notificationsDesc":
    "通知用于在您达到每日录制目标时以及有软件更新可用时提醒您。在 macOS 和 Windows 上，首次发送通知时操作系统会请求权限。",
  "perm.openNotificationsSettings": "打开通知设置",
  "perm.matrix": "权限概览",
  "perm.feature": "功能",
  "perm.matrixBluetooth": "Bluetooth（脑机接口设备）",
  "perm.matrixKeyboardMouse": "键盘和鼠标时间戳",
  "perm.matrixActiveWindow": "活动窗口跟踪",
  "perm.matrixNotifications": "通知",
  "perm.matrixNone": "无需权限",
  "perm.matrixAccessibility": "需要辅助功能权限",
  "perm.matrixOsPrompt": "首次使用时操作系统会提示",
  "perm.legendNone": "无需权限",
  "perm.legendRequired": "需要操作系统权限——未授权时功能会静默降级",
  "perm.legendPrompt": "首次使用时操作系统会提示",
  "perm.why": "{app} 为什么需要这些权限？",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "用于通过 BLE 发现和传输脑机接口头戴设备的数据。",
  "perm.whyAccessibility": "辅助功能",
  "perm.whyAccessibilityDesc":
    "用于记录键盘和鼠标事件的时间戳以提供活动上下文。仅存储事件发生的时间——不会记录输入内容或光标位置。",
  "perm.whyNotifications": "通知",
  "perm.whyNotificationsDesc": "用于在您达到每日录制目标和有更新可用时通知您。",
  "perm.privacyNote": `所有数据均存储在您的本地设备上，绝不会传输到任何服务器。您可以在"设置 → 活动跟踪"中禁用任何功能。`,
  "perm.screenRecording": "屏幕录制",
  "perm.screenRecordingDesc":
    "需要此权限才能捕获其他应用程序窗口以用于截图嵌入系统。未授予此权限时，macOS 会遮蔽窗口内容。",
  "perm.screenRecordingOk": "屏幕录制权限已授予。截图捕获将正常工作。",
  "perm.screenRecordingStep1": `打开"系统设置 → 隐私与安全性 → 屏幕与系统音频录制"`,
  "perm.screenRecordingStep2": "在列表中找到 NeuroSkill™ 并启用",
  "perm.screenRecordingStep3": "您可能需要退出并重新启动应用才能使更改生效",
  "perm.openScreenRecordingSettings": "打开屏幕录制设置",
  "perm.whyScreenRecording": "屏幕录制",
  "perm.whyScreenRecordingDesc":
    "用于捕获活动窗口以进行视觉相似性搜索和跨模态脑电图关联。仅存储主动选择的截图——不会持续录制。",
  "perm.matrixScreenRecording": "截图捕获",
  "perm.matrixScreenRecordingReq": "需要屏幕录制权限",
  "perm.calendar": "日历",
  "perm.calendarDesc": "日历工具可以读取事件以提供日程上下文。macOS 会在需要时请求权限。",
  "perm.requestCalendarPermission": "请求日历权限",
  "perm.openCalendarSettings": "打开日历隐私设置",
  "perm.location": "定位服务",
  "perm.locationDesc":
    "在 macOS 上，定位服务使用 CoreLocation（GPS / Wi-Fi / 蜂窝网络）进行高精度定位。在 Linux 和 Windows 上，应用使用基于 IP 的地理定位，无需权限。如果定位服务被拒绝或不可用，应用将自动回退到基于 IP 的地理定位。",
  "perm.locationOk": "定位权限已授予。将使用 CoreLocation 进行高精度定位。",
  "perm.locationFallback": "定位未授权——应用将使用基于 IP 的地理定位（城市级精度）。",
  "perm.locationStep1": `打开"系统设置 → 隐私与安全性 → 定位服务"`,
  "perm.locationStep2": "在列表中找到 {app} 并启用",
  "perm.locationStep3": "返回此页面——状态将自动更新",
  "perm.requestLocationPermission": "请求定位权限",
  "perm.openLocationSettings": "打开定位设置",
  "perm.whyLocation": "定位",
  "perm.whyLocationDesc":
    "用于向大语言模型提供精确的位置上下文，并将 GPS 定位数据与健康数据一起存储。被拒绝时将回退到基于 IP 的地理定位。",
  "perm.matrixLocation": "定位（GPS / IP）",
  "perm.matrixLocationReq": "定位服务（可选——回退到 IP 定位）",
  "perm.openInputMonitoringSettings": "打开输入监控设置",
  "perm.openFocusSettings": "打开专注模式设置",
  "perm.whyCalendar": "日历",
  "perm.whyCalendarDesc": "用于向大语言模型工具提供日程上下文，以便 AI 可以引用您即将到来的事件。",
  "perm.matrixCalendar": "日历事件",
  "perm.matrixCalendarReq": "需要日历访问权限",
  "perm.fullDiskAccess": "完全磁盘访问权限",
  "perm.fullDiskAccessDesc":
    "通过系统数据库直接检测专注模式所需。如未授权，应用将回退到较慢的旧方法。建议授权以确保勿扰模式集成的可靠性。",
  "perm.fullDiskAccessStep1": "打开系统设置 → 隐私与安全性 → 完全磁盘访问权限",
  "perm.fullDiskAccessStep2": "在列表中找到 NeuroSkill™（或运行守护进程的终端）并启用",
  "perm.fullDiskAccessStep3": "您可能需要退出并重新启动应用才能使更改生效",
  "perm.openFullDiskAccessSettings": "打开完全磁盘访问权限设置",
};

export default perm;
