import * as ksu from 'kernelsu';
import {parsePackageInfo, iconUrl, displayName} from './metadata.mjs';
const {exec, toast} = ksu;
import {validPackage, parseInstalled, parseDetached, commandForSelection, equalSets,
        appLabel, riskReasons, hookLabel} from './logic.mjs';
const cli='/data/adb/modules/zygisk-detach/detach';
const ui={
 list:document.getElementById('apps-list'), search:document.getElementById('search'),
 filter:document.getElementById('filter'), save:document.getElementById('apply'),
 undo:document.getElementById('undo'), backup:document.getElementById('backup'),
 restore:document.getElementById('restore'), refresh:document.getElementById('refresh'),
 diagnostics:document.getElementById('diagnostics'),
 select:document.getElementById('select-visible'), status:document.getElementById('status'),
 summary:document.getElementById('summary'), doctor:document.getElementById('doctor'),
 export:document.getElementById('export'), exportBox:document.getElementById('export-box')};
let installed=[], saved=new Set(), selected=new Set(), backupCount=0, busy=false, failed=false;
let labels=new Map();
async function readMetadata(packages) {
 // KernelSU 3 exposes this optional native PackageManager bridge. Unsupported
 // managers (including older KernelSU forks) use the original textual names.
 if (typeof ksu.getPackagesInfo !== 'function') return new Map();
 try {
  const result=await Promise.resolve(ksu.getPackagesInfo(packages.slice(0, 4096)));
  return parsePackageInfo(result, packages);
 } catch (_) { return new Map(); }
}
const labelFor=pkg=>displayName(pkg,labels,appLabel);
async function run(command) {
 const result=await exec(command);
 if (result.errno!==0) throw new Error((result.stderr||`exit ${result.errno}`).slice(0,360));
 return String(result.stdout||'');
}
function message(text,kind='neutral') {ui.status.textContent=text;ui.status.dataset.kind=kind;}
function updateControls() {
 const changed=!equalSets(saved,selected);
 const controls=[ui.save,ui.undo,ui.backup,ui.restore,ui.select,ui.refresh,ui.export,ui.diagnostics];
 for (const button of controls) button.disabled=busy;
 ui.save.disabled=busy||failed||!changed;
 ui.undo.disabled=busy||failed||!changed;
 ui.restore.disabled=busy||failed||!backupCount;
 ui.summary.textContent=`${selected.size} selected · ${installed.length} installed · ${[...saved].filter(p=>!installed.includes(p)).length} missing`;
 ui.save.textContent=changed?`Apply changes (${selected.size})`:'No pending changes';
}
function visiblePackages() {
 const query=ui.search.value.trim().toLowerCase();
 const all=[...new Set([...installed,...saved,...selected])].filter(validPackage);
 const mode=ui.filter.value;
 return all.filter(pkg=>(pkg.toLowerCase().includes(query)||labelFor(pkg).toLowerCase().includes(query))&&
 (mode==='all'||(mode==='selected'&&selected.has(pkg))||
  (mode==='missing'&&!installed.includes(pkg))||
  (mode==='available'&&installed.includes(pkg)))
 ).sort((a,b)=>(+selected.has(b)-+selected.has(a))||labelFor(a).localeCompare(labelFor(b)));
}
function draw() {
 const frag=document.createDocumentFragment();
 for (const pkg of visiblePackages()) {
  const row=document.createElement('label');row.className='app';
  const check=document.createElement('input');check.type='checkbox';check.checked=selected.has(pkg);
  check.disabled=busy||failed;
  check.addEventListener('change',()=>{if (check.checked) selected.add(pkg);else selected.delete(pkg);
   if(ui.filter.value!=='all') draw();else updateControls();});
  const icon=document.createElement('span');icon.className='app-icon';icon.setAttribute('aria-hidden','true');
  const monogram=labelFor(pkg).slice(0,2).toUpperCase();
  const src=installed.includes(pkg) ? iconUrl(pkg) : null;
  if (src) {
   const image=document.createElement('img');image.src=src;image.alt='';image.loading='lazy';
   image.addEventListener('error',()=>{image.remove();icon.textContent=monogram;},{once:true});
   icon.append(image); // manager-local scheme only; never remote image URLs
  } else icon.textContent=monogram;
  const info=document.createElement('span');info.className='app-info';
  const name=document.createElement('strong');name.textContent=labelFor(pkg);
  const id=document.createElement('small');id.textContent=pkg;
  info.append(name,id);
  if (!installed.includes(pkg)) {const hint=document.createElement('em');
   hint.textContent='Not currently installed · selection preserved';info.append(hint);}
  row.append(check,icon,info);frag.append(row);
 }
 ui.list.replaceChildren(frag);
 if (!ui.list.childElementCount) {const note=document.createElement('p');note.className='empty';
  note.textContent='No matching applications';ui.list.append(note);}
 updateControls();
}
async function refresh(force=false) {
 if(busy || (!force && !equalSets(selected,saved))) return;
 busy=true;updateControls();
 try {
  const [pkgText,detachText,reportText,backupText]=await Promise.all([
   run('pm list packages'),run(`${cli} list`),run(`${cli} doctor --json`),run(`${cli} backups`)]);
  const nextInstalled=parseInstalled(pkgText),nextSaved=new Set(parseDetached(detachText));
  const report=JSON.parse(reportText);
  const nextLabels=await readMetadata(nextInstalled);
  installed=nextInstalled;saved=nextSaved;selected=new Set(nextSaved);labels=nextLabels;
  backupCount=backupText.trim()?backupText.trim().split(/\r?\n/).length:0;
  failed=false;
  ui.doctor.replaceChildren();
  for (const [label,value] of [
   ['Hook',hookLabel(report)],['Android SDK',report.android_sdk||'unknown'],
   ['Root',report.root_manager||'unknown'],['Zygisk provider',report.zygisk_provider||'unknown'],
   ['Play Store PID',report.play_store_pid||'not running'],['Logcat',report.log_access?'available':'unavailable']]) {
   const cell=document.createElement('div');const heading=document.createElement('small');heading.textContent=label;
   const detail=document.createElement('strong');detail.textContent=String(value);
   cell.append(heading,detail);ui.doctor.append(cell);
  }
  message(`Configuration synchronized. ${hookLabel(report)}. Observations do not prove future update blocking.`,'neutral');
 } catch(e) {failed=true;message(`Could not refresh: ${e.message}. Check module and root permissions.`,'error');
 } finally {busy=false;draw();}
}
async function apply() {
 if(busy||failed||equalSets(saved,selected))return;
 const warnings=riskReasons([...selected]);
 const text=warnings.length?`${warnings.join('. ')}.\n\nProceed with these packages?`:
   `Replace configuration with ${selected.size} selected packages? Detachment can affect Play Store updates, subscriptions or app licensing.`;
 if(!window.confirm(text)) return;
 busy=true;updateControls();
 try {
  const command=commandForSelection([...selected]);
  await run(`${cli} ${command}`); // CLI writes atomically and makes a bounded recovery backup
  const actual=new Set(parseDetached(await run(`${cli} list`)));
  if(!equalSets(actual,selected)) throw new Error('read-back mismatch');
  saved=actual;selected=new Set(actual);
  backupCount=(await run(`${cli} backups`)).trim().split(/\r?\n/).filter(Boolean).length;
  message('Saved and read back successfully. Runtime hook still requires separate verification.','success');
  toast('Configuration saved');
 } catch(e) {message(`Apply not confirmed: ${e.message}. Use Refresh or Restore backup.`, 'error');
  toast('Configuration could not be verified');
 } finally {busy=false;draw();}
}
async function restore() {
 if(busy||failed||!backupCount||!window.confirm('Restore most recent previous configuration? A backup of current state will be retained.')) return;
 busy=true;updateControls();
 try {await run(`${cli} restore 0`);busy=false;await refresh(true);
  if(!failed)toast('Previous configuration restored');
 } catch(e) {busy=false;message(`Restore failed: ${e.message}`,'error');draw();}
}
ui.search.addEventListener('input',draw);
ui.filter.addEventListener('change',draw);
ui.apply.addEventListener('click',apply);
ui.undo.addEventListener('click',()=>{if(busy)return;selected=new Set(saved);draw();});
ui.refresh.addEventListener('click',()=>{
 if(!equalSets(saved,selected)&&!window.confirm('Discard pending changes and reload?'))return;
 refresh(true);
});
ui.restore.addEventListener('click',restore);
ui.backup.addEventListener('click',async()=>{if(busy)return;busy=true;updateControls();
 try {await run(`${cli} backup`);backupCount=Math.min(5,backupCount+1);message('Manual configuration backup created.','success');}
 catch(e){message(`Backup failed: ${e.message}`,'error');}finally{busy=false;updateControls();}});
ui.select.addEventListener('click',()=>{if(busy||failed)return;
 for(const pkg of visiblePackages().filter(p=>installed.includes(p)))selected.add(pkg);draw();});
ui.export.addEventListener('click',async()=>{if(busy)return;try{ui.exportBox.value=await run(`${cli} export`);
 ui.exportBox.hidden=false;message('Configuration exported below; nothing copied automatically.');}
 catch(e){message(`Export failed: ${e.message}`,'error');}});
ui.diagnostics.addEventListener('click',async()=>{if(busy)return;try{
 const text=await run(`${cli} doctor --json`);
 const data=JSON.parse(text);
 // Render internally validated, sanitized fields; never include package names or raw logs.
 ui.exportBox.value=JSON.stringify({module_installed:!!data.module_installed,
 configured_packages:Number(data.configured_packages)||0,android_sdk:Number(data.android_sdk)||0,
 root_manager:String(data.root_manager||'unknown'),zygisk_provider:String(data.zygisk_provider||'unknown'),
 hook_state:String(data.hook_state||'unverified'),log_access:!!data.log_access},null,2);
 ui.exportBox.hidden=false;message('Privacy-filtered diagnostics displayed, not copied or uploaded.');
 }catch(e){message(`Diagnostics unavailable: ${e.message}`,'error');}});
document.addEventListener('visibilitychange',()=>{if(!document.hidden)refresh(false);});
await refresh(true);
