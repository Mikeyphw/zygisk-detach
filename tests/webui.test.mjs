import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {parsePackageInfo, iconUrl, displayName} from '../ksu-webui/src/metadata.mjs';
import {validPackage,parseInstalled,parseDetached,commandForSelection,equalSets,
 appLabel,riskReasons,hookLabel} from '../ksu-webui/src/logic.mjs';
test('safe identifier requirements',()=>{
 assert.ok(validPackage('com.google.android.youtube'));
 for(const s of ['', 'abc','a..b','com.bad;id','com.bad name','com.bad/../oops', 'x'.repeat(130)+'.y'])assert.equal(validPackage(s),false);
});
test('listing is lossless and stable',()=>{
 assert.deepEqual(parseInstalled('package:com.a.one\npackage:com.a.one\npackage:org.b.two\nignored'),['com.a.one','org.b.two']);
 assert.deepEqual(parseDetached('org.b.two\ncom.a.one\n'),['com.a.one','org.b.two']);
});
test('commands cannot contain shell escapes',()=>{
 assert.equal(commandForSelection([]),'reset');
 assert.equal(commandForSelection(['com.b.two','com.a.one','com.a.one']),'detachall com.a.one com.b.two');
 assert.throws(()=>commandForSelection(['com.bad;touch /tmp/x']),/Invalid package/);
});
test('set equality and friendly label fallback',()=>{
 assert.ok(equalSets(new Set(['a','b']),new Set(['b','a'])));
 assert.ok(!equalSets(new Set(['a']),new Set(['b'])));
 assert.equal(appLabel('com.google.android.youtube'),'YouTube');
 assert.equal(appLabel('org.example.app'),'app');
});
test('system and licensing risk warnings',()=>{
 assert.ok(riskReasons(['com.android.vending']).length);
 assert.ok(riskReasons(['com.google.android.gms']).length);
 assert.deepEqual(riskReasons(['com.google.android.youtube']),[]);
});
test('hook labels do not conflate configuration with proof',()=>{
 assert.match(hookLabel({hook_state:'unverified'}),/unverified/);
 assert.match(hookLabel({hook_state:'observed_in_current_store_process'}),/observed/);
});
test('diagnostics UI exports only a privacy-filtered view',()=>{
 const html=readFileSync(new URL('../ksu-webui/src/index.html', import.meta.url),'utf8');
 const js=readFileSync(new URL('../ksu-webui/src/index.js', import.meta.url),'utf8');
 for(const id of ['diagnostics','backup','restore','undo','apply','export-box'])
  assert.match(html,new RegExp(`id="${id}"`));
 assert.match(js,/ui\.diagnostics\.addEventListener/);
 assert.match(js,/JSON\.stringify\(\{module_installed/);
 assert.doesNotMatch(js,/termux-clipboard-set/);
});
test('configuration readback is required before success',()=>{
 const js=readFileSync(new URL('../ksu-webui/src/index.js', import.meta.url),'utf8');
 assert.match(js,/read-back mismatch/);
 assert.match(js,/if\(!equalSets\(actual,selected\)\)/);
 assert.match(js,/if\(busy \|\| \(\!force && \!equalSets\(selected,saved\)\)\) return;/);
});

test('KernelSU package metadata labels and icons are strictly bounded',()=>{
 const list=['com.example.valid','com.example.app'];
 const data=[{packageName:'com.example.valid',appLabel:'Proper Label'},
  {packageName:'com.example.app',appLabel:'\u0001 Bad'},
  {packageName:'com.unexpected.other',appLabel:'Not installed'}];
 const labels=parsePackageInfo(data,list);
 assert.equal(labels.get('com.example.valid'),'Proper Label');
 assert.equal(labels.has('com.example.app'),false);
 assert.equal(labels.has('com.unexpected.other'),false);
 assert.equal(displayName('com.example.valid',labels,()=> 'fallback'),'Proper Label');
 assert.equal(iconUrl('com.example.valid'),'ksu://icon/com.example.valid');
 assert.equal(iconUrl('com.evil/x'),null);
 assert.deepEqual([...parsePackageInfo('{bad',list)],[]);
});
test('UI uses local KernelSU icon scheme, with a failure fallback',()=>{
 const js=readFileSync(new URL('../ksu-webui/src/index.js',import.meta.url),'utf8');
 assert.match(js,/getPackagesInfo/);
 assert.match(js,/image\.addEventListener\('error'/);
 assert.match(js,/iconUrl\(pkg\)/);
 assert.match(js,/labels=nextLabels/);
 assert.doesNotMatch(js,/https?:\/\/.*\$\{pkg\}/);
});
