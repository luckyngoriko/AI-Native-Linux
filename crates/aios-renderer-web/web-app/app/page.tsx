const ROUTES = [
  { href: "/", label: "Landing" },
  { href: "/recovery", label: "Recovery" },
] as const;

export default function Page() {
  return (
    <main>
      <h1>AIOS Web Renderer</h1>
      <nav>
        <ul>
          {ROUTES.map((r) => (
            <li key={r.href}>
              <a href={r.href}>{r.label}</a>
            </li>
          ))}
        </ul>
      </nav>
      <p>gRPC-Web client wiring lands in T-149.</p>
    </main>
  );
}
