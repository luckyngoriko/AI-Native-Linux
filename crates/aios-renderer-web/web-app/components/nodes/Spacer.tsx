import React from "react";

export interface SpacerProps {
  id: string;
}

const Spacer: React.FC<SpacerProps> = ({ id }) => (
  <div id={id} aria-hidden="true" />
);

export default Spacer;
