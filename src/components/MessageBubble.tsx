import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";

type MessageBubbleProps = {
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  variant?: "tool-process" | "final";
};

const useMarkdown = (role: "user" | "assistant", variant: "tool-process" | "final") =>
  role === "assistant" && variant === "final";

// Custom link component that opens in system browser
const CustomLink = ({ href, children }: { href?: string; children?: React.ReactNode }) => {
  const handleClick = async (e: React.MouseEvent<HTMLAnchorElement>) => {
    e.preventDefault();
    if (href) {
      try {
        await openUrl(href);
      } catch (error) {
        console.error('Failed to open URL:', error);
      }
    }
  };

  return (
    <a href={href} onClick={handleClick} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
};

export default function MessageBubble({
  role,
  content,
  streaming = false,
  variant = "final",
}: MessageBubbleProps) {
  const roleLabel =
    role === "user"
      ? "You"
      : variant === "tool-process"
        ? "Tool call"
        : "Assistant";
  const showContent = content || streaming;
  const renderMarkdown = useMarkdown(role, variant);
  const showRoleLabel = role === "assistant" && variant === "tool-process";
  return (
    <article
      className={`message-bubble ${role} ${variant}`}
      data-testid={`message-bubble-${role}`}
    >
      {showRoleLabel ? <p className="message-role">{roleLabel}</p> : null}
      {showContent ? (
        renderMarkdown ? (
          <div className="message-content markdown-body">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{
                a: ({ href, children }) => <CustomLink href={href}>{children}</CustomLink>
              }}
            >
              {content}
            </ReactMarkdown>
          </div>
        ) : (
          <p className="message-content">{content}</p>
        )
      ) : null}
      {streaming ? <p className="message-streaming">Streaming...</p> : null}
    </article>
  );
}
