import React from "react";

export interface FormProps {
  id: string;
  children?: React.ReactNode;
}

const Form: React.FC<FormProps> = ({ id, children }) => (
  <form id={id}>{children}</form>
);

export default Form;
