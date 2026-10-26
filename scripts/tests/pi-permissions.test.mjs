import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
// Execute the shipped extension's real guard; Typebox metadata construction is irrelevant here.
let source=await readFile(new URL('../../crates/endpoint-pi/resources/agentkit-control.mjs',import.meta.url),'utf8');
source=source.replace("import { Type } from '@sinclair/typebox';",'const Type = {Object: x=>x,Union:x=>x,Literal:x=>x,Optional:x=>x,String:x=>x};');
source=source.replace("import { createAssistantReading } from './agentkit-reading.mjs';",'const createAssistantReading = () => ({start:()=>"x",current:()=>null});');
source=source.replace("import { inputReceiptPayload, snapshotInputFiles } from './agentkit-input-files.mjs';",'const inputReceiptPayload=()=>({}), snapshotInputFiles=()=>({});');
const {default:configure}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
test('Pi tool guard enforces actual modes and approval answers, refusing malformed policies',async()=>{
 const root=await mkdtemp(join(tmpdir(),'pi-permission-'));const previous=process.env.AGENTKIT_PERMISSION_FILE;
 try {
 const file=join(root,'policy.json');process.env.AGENTKIT_PERMISSION_FILE=file;
 const handlers=new Map();configure({on:(name,fn)=>handlers.set(name,fn),registerTool(){},registerCommand(){}});
 const guard=handlers.get('tool_call');let asks=0,allowed=false;
 const ctx={ui:{confirm:async()=>{asks++;return allowed;}}};
 const event={toolName:'write',input:{path:'test.txt'}};
 for(const mode of ['allow-tools','deny-tools','ask-tools']){
 await writeFile(file,JSON.stringify({mode}));const result=await guard(event,ctx);
 if(mode==='allow-tools')assert.equal(result,undefined);else assert.equal(result.block,true);
 }
 assert.equal(asks,1);allowed=true;assert.equal(await guard(event,ctx),undefined);assert.equal(asks,2);
 for(const data of ['{}','invalid',JSON.stringify({mode:'danger-full-access'}),' '.repeat(2048)]){
 await writeFile(file,data);assert.equal((await guard(event,ctx)).block,true);
 }
 delete process.env.AGENTKIT_PERMISSION_FILE;assert.equal((await guard(event,ctx)).block,true);
 }finally{if(previous===undefined)delete process.env.AGENTKIT_PERMISSION_FILE;else process.env.AGENTKIT_PERMISSION_FILE=previous;await rm(root,{recursive:true,force:true});}
});
