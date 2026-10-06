<script lang="ts">
  import type { Settings } from './types';
  let { settings, onsettings }: {
    settings: Settings;
    onsettings: (settings: Settings) => void;
  } = $props();

  function change<Key extends keyof Settings>(key: Key, value: Settings[Key]) {
    onsettings({ ...settings, [key]: value });
  }
</script>

<div class="reading-settings">
  <label class="setting-field">
    <span>Typeface</span>
    <select aria-label="Typeface" value={settings.font_family} onchange={(event) => change('font_family', event.currentTarget.value as Settings['font_family'])}>
      <option value="inter">Inter</option>
      <option value="georgia">Georgia</option>
      <option value="system">System</option>
    </select>
  </label>
  <label class="setting-field">
    <span>Text size <output>{settings.font_size}px</output></span>
    <input aria-label="Text size" type="range" min="16" max="32" step="1" value={settings.font_size} oninput={(event) => change('font_size', Number(event.currentTarget.value))} />
  </label>
  <label class="setting-field">
    <span>Text weight <output>{settings.font_weight}</output></span>
    <input aria-label="Text weight" type="range" min="300" max="700" step="50" value={settings.font_weight} oninput={(event) => change('font_weight', Number(event.currentTarget.value))} />
  </label>
  <label class="setting-field">
    <span>Line spacing <output>{settings.line_height.toFixed(1)}</output></span>
    <input aria-label="Line spacing" type="range" min="1.4" max="2.2" step="0.1" value={settings.line_height} oninput={(event) => change('line_height', Number(event.currentTarget.value))} />
  </label>
  <label class="setting-field">
    <span>Paragraph spacing <output>{settings.paragraph_spacing.toFixed(1)}em</output></span>
    <input aria-label="Paragraph spacing" type="range" min="0.5" max="2.5" step="0.1" value={settings.paragraph_spacing} oninput={(event) => change('paragraph_spacing', Number(event.currentTarget.value))} />
  </label>
  <label class="setting-field">
    <span>Text brightness <output>{settings.text_brightness}%</output></span>
    <input aria-label="Text brightness" type="range" min="60" max="100" step="1" value={settings.text_brightness} oninput={(event) => change('text_brightness', Number(event.currentTarget.value))} />
  </label>
  <label class="setting-field">
    <span>Reading width <output>{settings.reading_width}px</output></span>
    <input aria-label="Reading width" type="range" min="600" max="1400" step="20" value={settings.reading_width} oninput={(event) => change('reading_width', Number(event.currentTarget.value))} />
  </label>
</div>
