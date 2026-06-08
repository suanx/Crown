import { cn } from "@/shared/lib/cn";

const PROVIDER_ICON: Record<string, string> = {
  deepseek: "deepseek",
  openai: "openai",
  anthropic: "anthropic",
  siliconflow: "siliconcloud",
  siliconcloud: "siliconcloud",
  ollama: "ollama",
  opencode: "opencode",
  xfyun: "xfyun",
  "openai-compatible": "generic",

};

export function ProviderIcon({
  providerId,
  name,
  size = 16,
  className,
}: {
  providerId: string;
  name?: string;
  size?: number;
  className?: string;
}) {
  const icon = PROVIDER_ICON[providerId.toLowerCase()] ?? "generic";
  return (
    <img
      src={`/icons/providers/${icon}.svg`}
      alt={name ?? providerId}
      title={name ?? providerId}
      width={size}
      height={size}
      className={cn("shrink-0 object-contain", className)}
      onError={(e) => {
        e.currentTarget.src = "/icons/providers/generic.svg";
      }}
    />
  );
}
