export type ImportSourceType = "text" | "csv" | "json" | "markdown";

export type ImportedFieldType =
  "text" | "image" | "number" | "boolean" | "date" | "null" | "object" | "array" | "mixed";

export type JsonValue = string | number | boolean | null | JsonValue[] | JsonObject;
export type JsonObject = { [key: string]: JsonValue };

export interface ImportedFieldDefinition {
  name: string;
  fieldType: ImportedFieldType;
  displayOrder: number;
  emptyCount: number;
  uniqueCount: number;
  samples: JsonValue[];
}

export interface ImportedItem {
  id: string;
  originalIndex: number;
  fields: JsonObject;
}

export interface ImportResult {
  sourceType: ImportSourceType;
  fields: ImportedFieldDefinition[];
  items: ImportedItem[];
  removedEmptyCount: number;
  duplicateCount: number;
  deduplicated: boolean;
}

export interface JsonArrayNode {
  pointer: string;
  itemCount: number;
}

export interface SavedFieldDefinition {
  id: string;
  name: string;
  fieldType: ImportedFieldType;
  displayOrder: number;
  isPrimaryIdentifier: boolean;
  isAuxiliaryIdentifier: boolean;
}

export interface DatasetOverview {
  id: string;
  name: string;
  sourceType: ImportSourceType;
  sourceFilename?: string;
  itemCount: number;
  fields: SavedFieldDefinition[];
}
