import { StreamingSettings } from '../bridge';
import { numericSettingRules, NumericSettingKey } from './constants';

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function normalizeNumericSetting(key: NumericSettingKey, value: number) {
  const rule = numericSettingRules[key];
  if (!Number.isFinite(value)) {
    return rule.min;
  }

  return clampNumber(Math.round(value), rule.min, rule.max);
}

export function normalizeSettings(settings: StreamingSettings): StreamingSettings {
  return {
    ...settings,
    width: normalizeNumericSetting('width', settings.width),
    height: normalizeNumericSetting('height', settings.height),
    fps: normalizeNumericSetting('fps', settings.fps),
    bitrateKbps: normalizeNumericSetting('bitrateKbps', settings.bitrateKbps),
    packetSize: normalizeNumericSetting('packetSize', settings.packetSize),
  };
}

export function validateSettings(settings: StreamingSettings) {
  return (Object.entries(numericSettingRules) as [NumericSettingKey, typeof numericSettingRules[NumericSettingKey]][])
    .flatMap(([key, rule]) => {
      const value = settings[key];
      if (!Number.isFinite(value)) {
        return [`${rule.label} must be a number.`];
      }
      if (!Number.isInteger(value)) {
        return [`${rule.label} must be a whole number.`];
      }
      if (value < rule.min || value > rule.max) {
        return [`${rule.label} must be between ${rule.min} and ${rule.max}.`];
      }
      return [];
    });
}
