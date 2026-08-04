import { useEffect, useState, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { createRoot } from 'react-dom/client';
import type { Root } from 'react-dom/client';
import type { CSSProperties, ReactNode } from 'react';
import DoodleButton from './DoodleButton';
import './DoodleModal.less';

export interface DoodleModalProps {
  open?: boolean;
  title?: ReactNode;
  onCancel?: () => void;
  footer?: ReactNode | null;
  children?: ReactNode;
  width?: number;
  onOk?: () => void;
  okText?: string;
  cancelText?: string;
  confirmLoading?: boolean;
  danger?: boolean;
}

function DoodleModal({
  open = false,
  title,
  onCancel,
  footer,
  children,
  width = 520,
  onOk,
  okText = '确 定',
  cancelText = '取 消',
  confirmLoading = false,
  danger = false,
}: DoodleModalProps) {
  const modalRef = useRef<HTMLDivElement>(null);
  const titleId = useRef(`doodle-modal-title-${Math.random().toString(36).slice(2, 9)}`).current;

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onCancel?.();
      return;
    }
    if (e.key === 'Tab') {
      const modal = modalRef.current;
      if (!modal) return;
      const focusable = modal.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      if (focusable.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }, [onCancel]);

  useEffect(() => {
    if (open) {
      const raf = requestAnimationFrame(() => {
        const modal = modalRef.current;
        if (!modal) return;
        const focusable = modal.querySelector<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        focusable?.focus();
      });
      return () => cancelAnimationFrame(raf);
    }
  }, [open]);

  if (!open) return null;

  const panelStyle: CSSProperties = width ? { width } : {};

  return createPortal(
    <div className="doodle-modal-overlay" onClick={onCancel} onKeyDown={handleKeyDown}>
      <div
        ref={modalRef}
        className="doodle-modal"
        style={panelStyle}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="doodle-modal-head">
          <span className="doodle-modal-title" id={titleId}>{title}</span>
          <span
            className="doodle-modal-close"
            onClick={onCancel}
            role="button"
            aria-label="close"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onCancel?.();
              }
            }}
          >
            ✕
          </span>
        </div>
        <div className="doodle-modal-body">{children}</div>
        {footer === null ? null : (
          <div className="doodle-modal-foot">
            {footer !== undefined ? (
              footer
            ) : (
              <>
                <DoodleButton size="small" type="ghost" onClick={onCancel}>
                  {cancelText}
                </DoodleButton>
                <DoodleButton
                  size="small"
                  type={danger ? 'danger' : 'primary'}
                  onClick={onOk}
                  disabled={confirmLoading}
                >
                  {okText}
                </DoodleButton>
              </>
            )}
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}

export interface DoodleModalStaticOptions {
  title?: ReactNode;
  content?: ReactNode;
  okText?: string;
  cancelText?: string;
  danger?: boolean;
  onOk?: () => void;
  onCancel?: () => void;
}

type StaticKind = 'confirm' | 'info' | 'error';

interface StaticState extends DoodleModalStaticOptions {
  kind: StaticKind;
  open: boolean;
  resolve?: (value: boolean) => void;
}

let staticState: StaticState = { kind: 'info', open: false };
let staticRoot: Root | null = null;
let staticHost: HTMLDivElement | null = null;
let staticFlush: (() => void) | null = null;

function ensureStaticMount() {
  if (!staticHost) {
    staticHost = document.createElement('div');
    document.body.appendChild(staticHost);
  }
  if (!staticRoot) staticRoot = createRoot(staticHost);
}

function syncStatic() {
  if (staticFlush) staticFlush();
  else if (staticRoot) staticRoot.render(<StaticModalRoot />);
}

function openStatic(
  partial: DoodleModalStaticOptions & { kind: StaticKind },
): Promise<boolean> {
  ensureStaticMount();
  return new Promise<boolean>((resolve) => {
    if (staticState.open) staticState.resolve?.(false);
    staticState = { ...partial, kind: partial.kind, open: true, resolve };
    syncStatic();
  });
}

function StaticModalRoot() {
  const [, setTick] = useState(0);

  useEffect(() => {
    staticFlush = () => setTick((n) => n + 1);
    return () => {
      staticFlush = null;
    };
  }, []);

  const s = staticState;
  if (!s.open) return null;

  const handleCancel = () => {
    const cb = s.onCancel;
    staticState = { ...s, open: false };
    syncStatic();
    s.resolve?.(false);
    cb?.();
  };

  const handleOk = () => {
    const cb = s.onOk;
    staticState = { ...s, open: false };
    syncStatic();
    s.resolve?.(true);
    cb?.();
  };

  const isConfirm = s.kind === 'confirm';
  const okText = s.okText || (isConfirm ? '确 定' : '好');
  const cancelText = s.cancelText || '取 消';
  const footer = isConfirm ? (
    undefined
  ) : (
    <DoodleButton size="small" type={s.kind === 'error' ? 'danger' : 'primary'} onClick={handleOk}>
      {okText}
    </DoodleButton>
  );

  return (
    <DoodleModal
      open
      title={s.title}
      onCancel={handleCancel}
      onOk={handleOk}
      okText={okText}
      cancelText={cancelText}
      danger={isConfirm ? !!s.danger : s.kind === 'error'}
      footer={footer}
    >
      {s.content}
    </DoodleModal>
  );
}

namespace DoodleModal {
  export function confirm(options: DoodleModalStaticOptions): Promise<boolean> {
    return openStatic({ ...options, kind: 'confirm' });
  }

  export function info(options: DoodleModalStaticOptions): Promise<boolean> {
    return openStatic({ ...options, kind: 'info' });
  }

  export function error(options: DoodleModalStaticOptions): Promise<boolean> {
    return openStatic({ ...options, kind: 'error', danger: true });
  }
}

export default DoodleModal;
