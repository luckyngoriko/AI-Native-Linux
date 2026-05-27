import React from "react";

export interface DividerProps {
  id: string;
}

const Divider: React.FC<DividerProps> = ({ id }) => <hr id={id} />;

export default Divider;
