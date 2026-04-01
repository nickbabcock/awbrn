import {
  ErrorComponent,
  Link,
  rootRouteId,
  useMatch,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router";

export function DefaultCatchBoundary({ error }: ErrorComponentProps) {
  const router = useRouter();
  const isRoot = useMatch({
    strict: false,
    select: (state) => state.id === rootRouteId,
  });

  console.error("Route error:", error);

  return (
    <div className="about-page">
      <div className="about-card">
        <div className="about-header">
          <h1 className="about-title">Route Error</h1>
        </div>
        <div className="about-body">
          <ErrorComponent error={error} />
          <div style={{ display: "flex", gap: "12px", flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => {
                void router.invalidate();
              }}
            >
              Try Again
            </button>
            {isRoot ? (
              <Link to="/">Home</Link>
            ) : (
              <button
                type="button"
                onClick={() => {
                  window.history.back();
                }}
              >
                Go Back
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
