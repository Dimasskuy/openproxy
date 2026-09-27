// views/providers/custom-model.ts — custom-model hook of the provider detail
// page. The form itself (modal, validation, submit) lives in
// components/model-custom-form.ts; the "Custom model" button in models.ts
// binds to the adapter exported here.

import { showCustomModelForm } from '../../components/model-custom-form.js';

export function onShowCustomModelForm(providerId: string): void {
  showCustomModelForm(providerId);
}
