// KernelSU WebUI package metadata is optional; all inputs are untrusted.
const PACKAGE = /^(?:[A-Za-z0-9_]+\.)+[A-Za-z0-9_]+$/;
export function isPackage(value) {
  return typeof value === 'string' && value.length <= 127 && PACKAGE.test(value);
}
export function parsePackageInfo(value, permitted) {
  const allowed = new Set([...permitted].filter(isPackage));
  let rows;
  if (typeof value === 'string') {
    try { rows = JSON.parse(value); } catch { return new Map(); }
  } else rows = value;
  if (!Array.isArray(rows)) return new Map();
  const out = new Map();
  for (const item of rows.slice(0, 4096)) {
    if (!item || typeof item !== 'object' || item.error ||
        !isPackage(item.packageName) || !allowed.has(item.packageName) ||
        typeof item.appLabel !== 'string') continue;
    const label = item.appLabel.trim();
    // Avoid pretending malformed bridge output is Android metadata.
    if (!label || label.length > 100 || /[\u0000-\u001f\u007f]/.test(label)) continue;
    out.set(item.packageName, label);
  }
  return out;
}
export function iconUrl(pkg) {
  // Never interpolate arbitrary strings into the manager's custom scheme.
  return isPackage(pkg) ? `ksu://icon/${pkg}` : null;
}
export function displayName(pkg, labels, fallback) {
  return labels?.get(pkg) ?? fallback(pkg);
}
