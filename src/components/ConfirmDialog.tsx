import { useLocaleStore } from "../stores/localeStore";

interface ConfirmDialogProps {
  isOpen: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export default function ConfirmDialog({
  isOpen,
  title,
  message,
  confirmText,
  cancelText,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const locale = useLocaleStore((state) => state.locale);
  
  const defaultConfirmText = locale === "zh" ? "确认删除" : "Delete";
  const defaultCancelText = locale === "zh" ? "取消" : "Cancel";

  if (!isOpen) return null;

  return (
    <div className="confirm-dialog-overlay" onClick={onCancel}>
      <div className="confirm-dialog" onClick={(e) => e.stopPropagation()}>
        <h3 className="confirm-dialog-title">{title}</h3>
        <p className="confirm-dialog-message">{message}</p>
        <div className="confirm-dialog-actions">
          <button
            className="confirm-dialog-btn confirm-dialog-btn-cancel"
            onClick={onCancel}
          >
            {cancelText || defaultCancelText}
          </button>
          <button
            className="confirm-dialog-btn confirm-dialog-btn-confirm"
            onClick={onConfirm}
          >
            {confirmText || defaultConfirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
