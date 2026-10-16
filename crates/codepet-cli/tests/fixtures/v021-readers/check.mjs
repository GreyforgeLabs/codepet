// Renders v0.3 bar-state.json exports through bar readers (spec §6.6, §9.2): a v0.2 bar or
// overlay still loaded during the upgrade window must degrade gracefully until the reload.
// The v0.2.1 bar reader is checked through summarize()/tooltip(); the v0.3 bar reader (no
// tooltips, SPEC-v0.3 §3) through its cell and its line, with petstate.js and Strings.js
// loaded from next to it.
// Usage: node check.mjs <CodePetBar.js> <BarState.js> <export.json>...
// Prints one JSON line per export; exits 1 if any reader refuses or throws.
import { existsSync, readFileSync } from 'node:fs';
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
const v03 = typeof bar.summarize !== 'function';
const sibling = (name) => barPath.replace(/CodePetBar\.js$/, name);
const petstate = v03 && existsSync(sibling('petstate.js')) ? load(sibling('petstate.js')) : null;
const strings = v03 && existsSync(sibling('Strings.js')) ? load(sibling('Strings.js')).STRINGS : null;
for (const file of exports) {
  const text = readFileSync(file, 'utf8');
  const out = { file: file.split('/').pop() };
  try {
    const doc = bar.parse(text);
    out.bar_parse = doc !== null;
    if (doc && !v03) {
      const cfg = bar.config({});
      const summary = bar.summarize(doc, cfg, {});
      out.label = summary.label;
      out.status = summary.status;
      out.accessible = typeof summary.accessibleName === 'string' && summary.accessibleName.length > 0;
      const tip = bar.tooltip(doc, cfg, {});
      out.tooltip = typeof tip === 'string' && tip.length > 0;
    } else if (doc) {
      const cfg = bar.config({});
      const face = petstate.sharedPriority(doc, Date.now(), {});
      const cell = bar.barCell(doc, face, null, cfg, false);
      const line = bar.faceLine(strings, null, doc, face);
      // The cell shows the name (or "Hi!" before Let's go!, whose line names the pet).
      out.label = [cell.label, line].filter(Boolean).join(' / ');
      out.status = face.state;
      out.accessible = line.length > 0 || cell.label.length > 0;
      out.tooltip = true; // v0.3 has no tooltips
    }
    const s = overlay.parse(text);
    out.overlay_parse = s !== null;
    if (s && typeof overlay.merge === 'function') {
      // v0.2.1 overlay: merges the file into its task snapshot.
      const merged = overlay.merge({ state_version: 0, db_id: s.db_id, care: {} }, s, false);
      out.merged_version = merged.state_version === s.state_version;
      out.merged_life = merged.life ? merged.life.name : null;
    } else if (s) {
      // v0.3 overlay (qml/omarchy/BarState.js): no snapshot to merge into; the desktop
      // pet draws the parsed document itself.
      out.merged_version = s.state_version === JSON.parse(text).state_version;
      out.merged_life = s.life ? s.life.name : null;
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
