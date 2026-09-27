// handlers/model-handlers/index.ts — facade re-exporting the public API of the sub-modules so
// consumers can keep importing from "./model-handlers.js". The original monolith was split into:
//   crud.ts — single-model CRUD (edit modal, update, delete, custom form)
//   bulk.ts — bulk enable/disable/test/delete on selected models
//   row.ts  — per-row toggle/test, multi-select, filter/sort/search

export {
  showEditModel,
  updateModel,
  deleteModel,
  createCustomModel,
  showCustomModelForm,
  closeCustomModelForm,
} from "./crud.js";

export {
  bulkEnableSelected,
  bulkDisableSelected,
  bulkTestSelected,
  bulkDeleteSelected,
} from "./bulk.js";

export {
  toggleModel,
  testModel,
  toggleModelSelection,
  toggleSelectAllModels,
  clearModelSelection,
  updateProviderFilter,
  updateAutoActivate,
  cycleProviderSort,
} from "./row.js";
