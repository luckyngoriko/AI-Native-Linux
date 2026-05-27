import type { Metadata } from "next";

export const metadata: Metadata = {
  title: "AIOS Renderer",
  description: "AIOS Web Renderer — L7 Interaction Layer",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>
        {children}
        {/* INV I2: closed shadow root attachment happens client-side in T-149 */}
        <div
          id="aios-chrome-shadow-root-host"
          style={{ position: "fixed", top: 0, left: 0, zIndex: 9999 }}
        />
      </body>
    </html>
  );
}
