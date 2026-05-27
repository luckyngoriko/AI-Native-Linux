import React from "react";

export interface CardProps {
  id: string;
  children?: React.ReactNode;
}

const Card: React.FC<CardProps> = ({ id, children }) => (
  <div id={id}>{children}</div>
);

export default Card;
