export interface MemoryMetrics {
  physicalTotal: number;
  physicalAvailable: number;
  commitUsed: number;
  commitLimit: number;
  systemCache: number;
  kernelPaged: number;
  kernelNonpaged: number;
  pagefileTotal: number | null;
  pagefileUsed: number | null;
  pagefileError: string | null;
  systemWorkingSet: number | null;
  systemWorkingSetPeak: number | null;
  systemWorkingSetError: string | null;
  lowMemory: boolean | null;
}

export type MemorySeries = "physical" | "pagefile" | "system";

export interface MemorySnapshot {
  sequence: number;
  timestampMs: number;
  elapsedMs: number;
  metrics: MemoryMetrics | null;
  error: string | null;
}

export interface Preferences {
  startWithWindows: boolean;
  startInTray: boolean;
  autoUpdateEnabled: boolean;
  cleanupRegions: CleanupAction[];
  warningPercent: number;
  dangerPercent: number;
  singleClickAction: TrayAction;
  middleClickAction: TrayAction;
  showCleanupResults: boolean;
  notificationSound: boolean;
  autoThresholdEnabled: boolean;
  autoThresholdPercent: number;
  autoIntervalEnabled: boolean;
  autoIntervalMinutes: number;
  hotkeyEnabled: boolean;
  hotkey: string;
}
export type TrayAction = "toggleWindow" | "cleanMemory" | "none";

export type CleanupAction =
  | "workingSet"
  | "systemFileCache"
  | "modifiedPageList"
  | "fullStandby"
  | "lowPriorityStandby"
  | "modifiedFileCache"
  | "registryCache"
  | "combineMemory";
export type CleanupStatus =
  | "completed"
  | "partial"
  | "cancelled"
  | "unsupported"
  | "permissionDenied"
  | "failed"
  | "unknown";
export interface RegionResult {
  region: CleanupAction;
  status: CleanupStatus;
  durationMs: number;
  errorCode: string | null;
}

export interface CleanupReport {
  id: string;
  regions: CleanupAction[];
  results: RegionResult[];
  status: CleanupStatus;
  startedAtMs: number;
  durationMs: number | null;
  before: MemorySnapshot | null;
  after: MemorySnapshot | null;
  message: string;
  errorCode: string | null;
  observations: { atSeconds: number; snapshot: MemorySnapshot }[];
}

export interface CleanupState {
  reports: CleanupReport[];
  busy: boolean;
  cooldownUntilMs: number;
}

export interface Capabilities {
  platform: string;
  build: number | null;
  cleanupSupported: boolean;
  reason: string | null;
  elevated: boolean;
}

export interface Bootstrap extends CleanupState {
  preferences: Preferences;
  snapshot: MemorySnapshot | null;
  history: MemorySnapshot[];
  capabilities: Capabilities;
  demo: boolean;
  settingsError: string | null;
}

export interface ProcessRow {
  pid: number;
  createdAt: number;
  name: string;
  bytes: number;
  metric: "privateWorkingSet" | "privateCommit";
}

export interface ProcessList {
  rows: ProcessRow[];
  inaccessible: number;
  timestampMs: number;
}

export type DemoScenario =
  | "normal"
  | "pressure"
  | "stale"
  | "cancelled"
  | "denied"
  | "unsupported"
  | "failed"
  | "partial"
  | "noChange"
  | "negative";

export interface Backend {
  bootstrap(): Promise<Bootstrap>;
  onSnapshot(callback: (snapshot: MemorySnapshot) => void): Promise<() => void>;
  onCleanup(callback: (state: CleanupState) => void): Promise<() => void>;
  onSettingsError?(callback: (error: string) => void): Promise<() => void>;
  getTopProcesses(): Promise<ProcessList>;
  savePreferences(preferences: Preferences): Promise<Preferences>;
  setPreferencesOpen?(open: boolean): Promise<void>;
  requestCleanup(regions: CleanupAction[], id: string): Promise<void>;
  setScenario?(scenario: DemoScenario): Promise<Bootstrap>;
}

export const cleanupOptions: {
  id: CleanupAction;
  label: string;
  hint: string;
}[] = [
  {
    id: "workingSet",
    label: "Working Set",
    hint: "Trims process working sets; applications may need to reload pages afterward.",
  },
  {
    id: "systemFileCache",
    label: "System File Cache",
    hint: "Trims the system file-cache working set; later reads may be slower.",
  },
  {
    id: "modifiedPageList",
    label: "Modified Page List",
    hint: "Writes modified pages to disk; may cause a pause.",
  },
  {
    id: "fullStandby",
    label: "Standby List",
    hint: "Clears all standby pages, including useful cache; later reads may be slower.",
  },
  {
    id: "lowPriorityStandby",
    label: "Standby List (Without Priority)",
    hint: "Clears priority-0 standby pages.",
  },
  {
    id: "modifiedFileCache",
    label: "Modified File Cache",
    hint: "Flushes buffered writes on local fixed volumes; may cause disk activity.",
  },
  {
    id: "registryCache",
    label: "Registry Cache (Win 8.1+)",
    hint: "Flushes modified registry hives.",
  },
  {
    id: "combineMemory",
    label: "Combine Memory Lists (Win 10+)",
    hint: "Asks Windows to combine identical physical memory pages.",
  },
];
export const defaultPreferences: Preferences = {
  startWithWindows: false,
  startInTray: false,
  autoUpdateEnabled: false,
  warningPercent: 70,
  dangerPercent: 90,
  singleClickAction: "toggleWindow",
  middleClickAction: "cleanMemory",
  showCleanupResults: true,
  notificationSound: true,
  autoThresholdEnabled: false,
  autoThresholdPercent: 80,
  autoIntervalEnabled: false,
  autoIntervalMinutes: 90,
  hotkeyEnabled: false,
  hotkey: "Ctrl+F1",
  cleanupRegions: [
    "workingSet",
    "systemFileCache",
    "lowPriorityStandby",
    "modifiedFileCache",
    "registryCache",
    "combineMemory",
  ],
};
