import React from "react";

export interface CodeBlockProps {
  id: string;
  children?: React.ReactNode;
}

const CodeBlock: React.FC<CodeBlockProps> = ({ id, children }) => (
  <pre id={id}>
    <code>{children}</code>
  </pre>
);

export default CodeBlock;
