import { downloadBackup } from "../views/config/backup.js";

export function exportConfig(): void {
  void downloadBackup();
}
