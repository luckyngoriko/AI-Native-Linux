import React from "react";

export interface AgentMessageProps {
  id: string;
  children?: React.ReactNode;
}

const AgentMessage: React.FC<AgentMessageProps> = ({ id, children }) => (
  <div id={id}>{children}</div>
);

export default AgentMessage;
