import React from "react";

export interface ActionButtonProps {
  id: string;
  children?: React.ReactNode;
}

const ActionButton: React.FC<ActionButtonProps> = ({ id, children }) => (
  <button id={id} type="button">
    {children}
  </button>
);

export default ActionButton;
