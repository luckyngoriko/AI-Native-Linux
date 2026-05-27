import React from "react";

export interface HeadingProps {
  id: string;
  children?: React.ReactNode;
}

const Heading: React.FC<HeadingProps> = ({ id, children }) => (
  <h2 id={id}>{children}</h2>
);

export default Heading;
