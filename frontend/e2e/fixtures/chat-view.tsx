import React, {useState} from 'react';
import {createRoot} from 'react-dom/client';
import {initializeControlSession} from '../../src/api/controlSession';
import ChatView from '../../src/components/chat/ChatView';
import {useApprovalStore} from '../../src/stores/approvalStore';
const test=window.__chat={messages:[],events:[],anchor:null,send:null,handler:null};
function Harness(){
 const [id,setId]=useState<string|null>(null),[revision,setRevision]=useState(0);
 test.switchTo=setId;test.refresh=()=>setRevision(v=>v+1);
 test.approvals=()=>useApprovalStore.getState().approvals;
 const [voice,setVoice]=useState(null);
 const anchor=React.useCallback(value=>{test.anchor=value;setVoice(value);},[]);
 return <><output id="anchor">{JSON.stringify(voice)}</output><ChatView conversationId={id} conversationRefreshRevision={revision} onConversationChange={setId}
 onVoiceAnchorChange={anchor} showConversationToggle={false} showExecutionToggle={false}
 onToggleConversations={()=>{}} onToggleExecution={()=>{}} onOpenCurrentTask={()=>{}}
 renderExecution={value=>{test.execution=value;return <output id="execution">{JSON.stringify(value.state)}</output>;}}/></>;
}
await initializeControlSession();
createRoot(document.getElementById('root')!).render(<Harness/>);
