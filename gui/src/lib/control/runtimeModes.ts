import { ControlApiError, type ModeTransitionPlan } from '../types';

export type PlannedMode = {
 id: string;
 label: string;
 description: string;
 flowgraphGroups: string[];
 managedApps: string[];
 notifications: string;
};

export type DisplayMode = PlannedMode & {
 configured: boolean;
};

export type ManagedDirectiveRow = {
 label: string;
 values: string[];
};

export const plannedModes: PlannedMode[] = [
 {
  id: 'daily',
  label: 'Daily',
  description: 'Assistant, alerts, and lightweight personal automations.',
  flowgraphGroups: ['assistant', 'alerts', 'rss'],
  managedApps: ['stop obs', 'stop warudo', 'stop tts'],
  notifications: 'normal',
 },
 {
  id: 'streaming',
  label: 'Streaming',
  description: 'Streaming stack, avatar tools, OBS control, and stream-safe alerts.',
  flowgraphGroups: ['assistant', 'streaming', 'avatar', 'obs'],
  managedApps: ['start obs', 'start warudo', 'start tts'],
  notifications: 'stream_safe',
 },
 {
  id: 'work',
  label: 'Work',
  description: 'Reduced interruptions while keeping critical alerts and assistant access.',
  flowgraphGroups: ['assistant', 'alerts'],
  managedApps: ['stop obs', 'stop warudo', 'stop tts'],
  notifications: 'important_only',
 },
 {
  id: 'sleep',
  label: 'Sleep',
  description: 'Only critical monitoring and emergency notification flows.',
  flowgraphGroups: ['emergency_alerts'],
  managedApps: ['stop obs', 'stop warudo', 'stop tts'],
  notifications: 'critical_only',
 },
 {
  id: 'rta',
  label: 'RTA',
  description: 'Game/run-specific timers, splits, alerts, and reduced background noise.',
  flowgraphGroups: ['game', 'timer', 'alerts'],
  managedApps: ['leave game tools', 'stop streaming extras'],
  notifications: 'run_safe',
 },
];

export function buildDisplayModes(configuredModeIds: string[]): DisplayMode[] {
 const configured = new Set(configuredModeIds);
 const known = new Set(plannedModes.map((m) => m.id));
 const plannedRows = plannedModes.map((m) => ({ ...m, configured: configured.has(m.id) }));
 const customRows = configuredModeIds
  .filter((id) => !known.has(id))
  .map((id) => ({
   id,
   label: id,
   description: 'Configured runtime mode from conf.',
   flowgraphGroups: ['configured'],
   managedApps: ['use configured desired state'],
   notifications: 'configured',
   configured: true,
  }));
 return [...customRows, ...plannedRows];
}

export function managedDirectiveRows(plan: ModeTransitionPlan | null): ManagedDirectiveRow[] {
 if (!plan) return [];
 return [
  { label: 'Start', values: plan.target_managed_apps.start },
  { label: 'Stop', values: plan.target_managed_apps.stop },
  { label: 'Minimize', values: plan.target_managed_apps.minimize },
  { label: 'Leave', values: plan.target_managed_apps.leave },
 ].filter((row) => row.values.length > 0);
}

export function formatControlApiError(e: unknown): string {
 if (e instanceof ControlApiError) return `${e.status} ${e.statusText}`;
 if (e instanceof Error) return e.message;
 return String(e);
}

export function joinList(values: string[]): string {
 return values.length > 0 ? values.join(', ') : '-';
}
