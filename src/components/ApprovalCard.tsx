type ApprovalCardProps = {
  approvalId: string;
  actionType: string;
  path: string;
  content?: string;
  busy?: boolean;
  onApprove: (approvalId: string) => Promise<void>;
  onReject: (approvalId: string) => Promise<void>;
};

const CONTENT_PREVIEW_MAX_CHARS = 200;

function previewText(content: string) {
  if (content.length <= CONTENT_PREVIEW_MAX_CHARS) {
    return content;
  }
  return `${content.slice(0, CONTENT_PREVIEW_MAX_CHARS)}...`;
}

export default function ApprovalCard({
  approvalId,
  actionType,
  path,
  content,
  busy = false,
  onApprove,
  onReject,
}: ApprovalCardProps) {
  return (
    <div className="approval-card" data-testid={`approval-card-${approvalId}`}>
      <p className="approval-title">Pending approval: {actionType}</p>
      <p className="approval-path">Path: {path || "(missing path)"}</p>
      {actionType === "write_file" && content !== undefined ? (
        <pre className="approval-preview">{previewText(content)}</pre>
      ) : null}
      <div className="approval-actions">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void onApprove(approvalId);
          }}
        >
          Accept
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void onReject(approvalId);
          }}
        >
          Reject
        </button>
      </div>
    </div>
  );
}
