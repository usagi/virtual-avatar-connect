import type { FlowgraphCapability, FlowgraphEffectClass, FlowgraphNodeSpec } from '../types';

export function effectClassLabel(effectClass: FlowgraphEffectClass | undefined): string {
 switch (effectClass) {
  case 'pure':
   return 'Pure';
  case 'stateful':
   return 'Stateful';
  case 'effectful':
   return 'Effect';
  default:
   return 'Unknown';
 }
}

export function capabilityLabel(capability: FlowgraphCapability): string {
 switch (capability) {
  case 'file_read':
   return 'File read';
  case 'file_write':
   return 'File write';
  case 'file_watch':
   return 'File watch';
  case 'network':
   return 'Network';
  case 'db_read':
   return 'DB read';
  case 'db_write':
   return 'DB write';
  case 'process_control':
   return 'Process';
  case 'window_control':
   return 'Window';
  case 'desktop_capture':
   return 'Capture';
  case 'desktop_notification':
   return 'Notify';
  case 'obs_control':
   return 'OBS';
  case 'twitch_api':
   return 'Twitch';
  case 'credential_access':
   return 'Credential';
  case 'audio_output':
   return 'Audio';
  case 'trace_write':
   return 'Trace';
  default:
   return String(capability);
 }
}

export function capabilitySummary(spec: FlowgraphNodeSpec | undefined, limit = 3): string {
 const capabilities = spec?.capabilities ?? [];
 if (capabilities.length === 0) return '';
 const shown = capabilities.slice(0, limit).map(capabilityLabel);
 const rest = capabilities.length - shown.length;
 return rest > 0 ? `${shown.join(' / ')} +${rest}` : shown.join(' / ');
}

export function effectTooltip(spec: FlowgraphNodeSpec | undefined): string {
 if (!spec) return '';
 const parts = [`Effect class: ${effectClassLabel(spec.effect_class)}`];
 const caps = capabilitySummary(spec, 10);
 if (caps) parts.push(`Capabilities: ${caps}`);
 return parts.join('\n');
}
