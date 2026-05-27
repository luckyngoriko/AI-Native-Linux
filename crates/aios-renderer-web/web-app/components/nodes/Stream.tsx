import React from "react";

export interface StreamProps {
  id: string;
  children?: React.ReactNode;
}

const Stream: React.FC<StreamProps> = ({ id, children }) => (
  <div id={id}>{children}</div>
);

export default Stream;
