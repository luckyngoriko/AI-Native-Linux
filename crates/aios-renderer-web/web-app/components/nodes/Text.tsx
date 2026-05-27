import React from "react";

export interface TextProps {
  id: string;
  children?: React.ReactNode;
}

const Text: React.FC<TextProps> = ({ id, children }) => (
  <span id={id}>{children}</span>
);

export default Text;
