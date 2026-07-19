import { cn } from "@/lib/utils";

export function DiffBlock({ text }: { text: string }) {
  return (
    <pre className="mt-1 overflow-x-auto rounded-md bg-muted p-2 font-mono text-xs">
      {text.split("\n").map((line, i) => (
        <div
          key={i}
          className={cn(
            line.startsWith("+") && "text-green-600 dark:text-green-400",
            line.startsWith("-") && "text-red-600 dark:text-red-400",
          )}
        >
          {line || " "}
        </div>
      ))}
    </pre>
  );
}
