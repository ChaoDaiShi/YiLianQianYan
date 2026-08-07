import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import ChatView from "../components/chat/ChatView";
import ConversationSidebar from "../components/chat/ConversationSidebar";

export default function ChatPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const conversationId = id || null;
  const [showSidebar, setShowSidebar] = useState(true);

  const onConversationChange = (nextId: string | null) => {
    if (nextId) navigate(`/chat/${nextId}`);
    else navigate("/chat");
  };

  return (
    <div className="flex h-full">
      {showSidebar && (
        <div className="w-64 border-r border-[var(--border)] flex-shrink-0 bg-[var(--panel)]/50 backdrop-blur-sm">
          <ConversationSidebar
            activeId={conversationId}
            onSelect={(cid) => onConversationChange(cid)}
            onNew={() => onConversationChange(null)}
          />
        </div>
      )}
      <div className="flex-1 flex flex-col min-w-0">
        <ChatView
          conversationId={conversationId}
          onConversationChange={onConversationChange}
          showSidebar={showSidebar}
          onToggleSidebar={() => setShowSidebar(!showSidebar)}
        />
      </div>
    </div>
  );
}
