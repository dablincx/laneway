import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

// Card text is rendered here in the browser: no extra request, no work for the server. Raw
// HTML in the text is never rendered (skipHtml), and react-markdown drops unsafe link targets
// such as `javascript:`. Load this lazily (see MarkdownView) so the board doesn't pay for it.

const components: Components = {
  a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noopener noreferrer nofollow" />,
  // Task list checkboxes are shown, not edited here.
  input: ({ node: _node, ...props }) => <input {...props} disabled />,
};

export default function Markdown({ text }: { text: string }) {
  return (
    <div className="markdown text-sm">
      <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
}
