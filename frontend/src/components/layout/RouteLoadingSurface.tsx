import Skeleton from "../ui/Skeleton";

export default function RouteLoadingSurface() {
  return (
    <section
      className="flex h-full min-h-0 items-center justify-center p-6"
      aria-label="页面加载中"
    >
      <div className="w-full max-w-3xl rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)]/80 p-6 shadow-[var(--shadow-sm)]">
        <Skeleton className="h-7 w-40" />
        <Skeleton className="mt-3 h-4 w-64" />
        <Skeleton className="mt-8 h-32 w-full" />
      </div>
    </section>
  );
}
