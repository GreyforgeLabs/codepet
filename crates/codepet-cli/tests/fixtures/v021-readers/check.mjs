// Renders v0.3 bar-state.json exports through bar readers (spec §6.6, §9.2): a v0.2 bar or
// overlay still loaded during the upgrade window must degrade gracefully until the reload.
// Usage: node check.mjs <CodePetBar.js> <BarState.js> <export.json>...
// Prints one JSON line per export; exits 1 if any reader refuses or throws.
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

function load(path) {
  const ctx = {};
  vm.createContext(ctx);
  // QML JavaScript libraries start with `.pragma library`; plain JS otherwise.
  const source = readFileSync(path, 'utf8').replace(/^\.pragma library$/m, '');
  vm.runInContext(source, ctx, { filename: path });
  return ctx;
}

const [barPath, overlayPath, ...exports] = process.argv.slice(2);
const bar = load(barPath);
const overlay = load(overlayPath);
for (const file of exports) {
  const text = readFileSync(file, 'utf8');
  const out = { file: file.split('/').pop() };
  try {
    const doc = bar.parse(text);
    out.bar_parse = doc !== null;
    if (doc) {
      const cfg = bar.config({});
      const summary = bar.summarize(doc, cfg, {});
      out.label = summary.label;
      out.status = summary.status;
      out.accessible = typeof summary.accessibleName === 'string' && summary.accessibleName.length > 0;
      const tip = bar.tooltip(doc, cfg, {});
      out.tooltip = typeof tip === 'string' && tip.length > 0;
    }
    const s = overlay.parse(text);
    out.overlay_parse = s !== null;
    if (s) {
      const merged = overlay.merge({ state_version: 0, db_id: s.db_id, care: {} }, s, false);
      out.merged_version = merged.state_version === s.state_version;
      out.merged_life = merged.life ? merged.life.name : null;
    }
  } catch (e) {
    out.error = String(e && e.stack ? e.stack.split('\n')[0] : e);
  }
  const ok = out.bar_parse && out.overlay_parse && out.accessible && out.tooltip
    && out.merged_version && !out.error;
  out.ok = !!ok;
  console.log(JSON.stringify(out));
  if (!ok) process.exitCode = 1;
}
