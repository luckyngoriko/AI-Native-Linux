import React from "react";

export interface ApprovalPromptProps {
  id: string;
  children?: React.ReactNode;
}

/**
 * chrome-zone: closed shadow root rendering happens at the
 * layout/composition level; the component itself just emits the markup.
 */
const ApprovalPrompt: React.FC<ApprovalPromptProps> = ({ id, children }) => (
  <div id={id} data-chrome-zone="true">
    {children}
  </div>
);

export default ApprovalPrompt;
