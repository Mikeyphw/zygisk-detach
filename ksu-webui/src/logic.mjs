// Pure helpers, also exercised in hosted tests. Never shell-interpolate untrusted input.
const known = Object.freeze({
  'com.android.vending': 'Google Play Store',
  'com.google.android.gms': 'Google Play services',
  'com.google.android.youtube': 'YouTube',
  'com.google.android.apps.youtube.music': 'YouTube Music',
  'com.android.chrome': 'Chrome',
  'com.instagram.android': 'Instagram',
  'org.telegram.messenger': 'Telegram',
  'com.whatsapp': 'WhatsApp',
});
export function validPackage(name) {
  return typeof name === 'string' && name.length > 0 && name.length <= 127 && name.includes('.') &&
    name.split('.').every(part => part.length && /^[A-Za-z0-9_]+$/.test(part));
}
export function parseInstalled(raw) {
  return [...new Set(String(raw || '').split(/\r?\n/).map(s => s.trim())
    .filter(s => s.startsWith('package:')).map(s => s.slice(8)).filter(validPackage))].sort();
}
export function parseDetached(raw) {
  return [...new Set(String(raw || '').split(/\r?\n/).map(s => s.trim()).filter(validPackage))].sort();
}
export function commandForSelection(names) {
  const values = [...new Set(names)].sort();
  if (!values.every(validPackage)) throw new Error('Invalid package identifier');
  return values.length ? `detachall ${values.join(' ')}` : 'reset';
}
export function equalSets(a,b) {return a.size===b.size && [...a].every(v=>b.has(v));}
export function appLabel(pkg) {return known[pkg] || pkg.split('.').slice(-1)[0].replaceAll('_',' ');}
export function riskReasons(names) {
  const out=[];
  const list=[...names];
  if (list.includes('com.android.vending')) out.push('Google Play Store itself is selected');
  if (list.some(p=>p==='com.google.android.gms'||p==='com.google.android.gsf'))
    out.push('Google system services may be involved in app licensing and integrity');
  if (list.some(p=>p.startsWith('com.android.') && p!=='com.android.chrome'))
    out.push('A system application is selected');
  return out;
}
export function hookLabel(report) {
  switch(report?.hook_state) {
    case 'observed_in_current_store_process': return 'Binder interception observed in current Play Store process';
    case 'installed_no_matching_query_observed': return 'Hook installed; no matching query observed';
    case 'play_store_not_running': return 'Play Store is not running';
    default: return 'Hook unverified';
  }
}
