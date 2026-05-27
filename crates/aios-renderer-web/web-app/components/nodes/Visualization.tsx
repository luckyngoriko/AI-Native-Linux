import React from "react";

export interface VisualizationProps {
  id: string;
  children?: React.ReactNode;
}

const Visualization: React.FC<VisualizationProps> = ({ id, children }) => (
  <div id={id}>{children}</div>
);

export default Visualization;
