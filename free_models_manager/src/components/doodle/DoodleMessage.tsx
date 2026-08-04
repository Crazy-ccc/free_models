import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { Root } from 'react-dom/client';
import type { ReactNode } from 'react';
import './DoodleMessage.less';

type DoodleMessageType = 'success' | 'error' | 'warning';

interface MessageItem {
  id: number;
  type: DoodleMessageType;
  content: ReactNode;
}

let messageItems: MessageItem[] = [];
let messageRoot: Root | null = null;
let messageHost: HTMLDivElement | null = null;
let messageFlush: (() => void) | null = null;
let messageSeq = 0;

function ensureMessageMount() {
  if (!messageHost) {
    messageHost = document.createElement('div');
    document.body.appendChild(messageHost);
  }
  if (!messageRoot) messageRoot = createRoot(messageHost);
}

function syncMessage() {
  if (messageFlush) messageFlush();
  else if (messageRoot) messageRoot.render(<MessageRoot />);
}

function openMessage(type: DoodleMessageType, content: ReactNode) {
  ensureMessageMount();
  const id = ++messageSeq;
  messageItems = [...messageItems, { id, type, content }];
  syncMessage();
  window.setTimeout(() => {
    messageItems = messageItems.filter((item) => item.id !== id);
    syncMessage();
  }, 1600);
}

function MessageRoot() {
  const [, setTick] = useState(0);

  useEffect(() => {
    messageFlush = () => setTick((n) => n + 1);
    return () => {
      messageFlush = null;
    };
  }, []);

  return (
    <div className="doodle-message-root">
      {messageItems.map((item) => (
        <div key={item.id} className={`doodle-message doodle-message-${item.type}`}>
          {item.content}
        </div>
      ))}
    </div>
  );
}

const DoodleMessage = {
  success(content: ReactNode) {
    openMessage('success', content);
  },
  error(content: ReactNode) {
    openMessage('error', content);
  },
  warning(content: ReactNode) {
    openMessage('warning', content);
  },
  destroy() {
    messageItems = [];
    syncMessage();
  },
};

export default DoodleMessage;