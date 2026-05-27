import React from "react";

export interface InlineCodeProps {
  id: string;
  children?: React.ReactNode;
}

const InlineCode: React.FC<InlineCodeProps> = ({ id, children }) => (
  <code id={id}>{children}</code>
);

export default InlineCode;
