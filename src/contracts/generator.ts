export type FileOwnership = "generated" | "user";

export interface ManifestFile { owner: FileOwnership; hash: string; }
export interface GenerationManifest {
  formatVersion: number;
  templateId: string;
  templateVersion: string;
  blueprintHash: string;
  files: Record<string, ManifestFile>;
}
export interface GenerationConflict { path: string; artifactPath: string; reason: string; }
export interface GenerationPreview {
  templateId: string;
  templateVersion: string;
  targetDirectory: string;
  entityCount: number;
  generatedFileCount: number;
  userFileCount: number;
  files: string[];
}
export interface GenerationResult {
  templateId: string;
  templateVersion: string;
  targetDirectory: string;
  blueprintHash: string;
  writtenFileCount: number;
  preservedFileCount: number;
  conflicts: GenerationConflict[];
  manifest: GenerationManifest;
}
