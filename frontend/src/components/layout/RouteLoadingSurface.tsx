export default function RouteLoadingSurface() {
  return (
    <section
      className="route-loading-surface flex h-full min-h-0 items-center justify-center p-6"
      aria-label="页面加载中"
    >
      <div className="route-loading-placeholder w-full max-w-3xl" aria-hidden="true">
        <span className="route-loading-line route-loading-line-title" />
        <span className="route-loading-line route-loading-line-copy" />
      </div>
    </section>
  );
}
