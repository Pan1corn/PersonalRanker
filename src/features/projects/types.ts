export interface ProjectMetadata {
  id: string;
  name: string;
  createdAt: string;
  lastOpenedAt: string;
  dataFormatVersion: number;
  projectPath: string;
}

export interface AutosaveStatus {
  sessionOpen: boolean;
  recoveredUncleanSession: boolean;
  lastAutosaveAt?: string;
  lastAutosaveAction?: string;
}

export interface OpenProjectResult {
  project: ProjectMetadata;
  autosave: AutosaveStatus;
}

export interface ProjectSnapshot {
  id: string;
  snapshotType:
    | "import_completed"
    | "comparison_completed"
    | "slider_completed"
    | "sort_confirmed"
    | "score_confirmed"
    | "export_completed"
    | "before_restore";
  label: string;
  sourceTaskId?: string;
  sourceDatasetId?: string;
  createdAt: string;
  sizeBytes: number;
}

export interface RestoreSnapshotResult {
  snapshot: ProjectSnapshot;
  autosave: AutosaveStatus;
}
