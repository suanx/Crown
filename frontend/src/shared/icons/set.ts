/**
 * ============================================================================
 * Icon Set — 精选 Phosphor Icons
 * ============================================================================
 *
 * 选 Phosphor 而不是 Lucide / Heroicons 的理由:
 *   - 6 种 weight (thin/light/regular/bold/fill/duotone),设计感更精致
 *   - 几何骨架更细腻,不大众脸
 *   - 与 Claude / Notion / Linear 等品牌走相似审美路线
 *
 * 集中管理 — UI 组件 import IconName 而不是直接 import phosphor.
 * 换图标库时只改这一文件.
 * ----------------------------------------------------------------------------
 */

export {
  // 导航 / 操作
  List as MenuIcon,
  SidebarSimple as SidebarIcon,
  MagnifyingGlass as SearchIcon,
  Plus as PlusIcon,
  PencilSimple as EditIcon,
  Trash as TrashIcon,
  Star as StarIcon,
  PushPin as PinIcon,
  DotsThree as MoreIcon,
  DotsThreeVertical as MoreVerticalIcon,
  ArrowRight as ArrowRightIcon,
  ArrowUp as SendIcon,
  Stop as StopIcon,
  X as CloseIcon,
  Check as CheckIcon,
  CheckCircle as CheckCircleIcon,
  Warning as WarningIcon,
  WarningCircle as WarningCircleIcon,
  Info as InfoIcon,
  CaretDown as CaretDownIcon,
  CaretRight as CaretRightIcon,
  CaretUp as CaretUpIcon,
  Copy as CopyIcon,
  ArrowsClockwise as RefreshIcon,
  ArrowSquareOut as ExternalLinkIcon,

  // 应用功能
  ChatCircle as ChatIcon,
  Question as QuestionIcon,
  Folders as ProjectsIcon,
  PuzzlePiece as SkillIcon,
  Plug as McpIcon,
  ChartBar as StatsIcon,
  GearSix as SettingsIcon,
  Sparkle as BrandIcon,
  Robot as AgentIcon,
  Brain as ReasoningIcon,
  Code as CodeIcon,
  TerminalWindow as TerminalIcon,
  Wrench as ToolIcon,
  Lightning as FlashIcon,
  Shield as ShieldIcon,
  Lock as LockIcon,
  Globe as GlobeIcon,
  FolderOpen as FolderIcon,
  File as FileIcon,
  FileMagnifyingGlass as FileSearchIcon,
  GitDiff as DiffIcon,
  SquareSplitHorizontal as SwapToRightIcon,
  Stack as StackIcon,
  Download as DownloadIcon,
  Upload as UploadIcon,
  Paperclip as AttachIcon,
  Sun as SunIcon,
  Moon as MoonIcon,
  Desktop as SystemIcon,
  Bug as BugIcon,
  Hammer as BuildIcon,
  TestTube as TestIcon,
  CurrencyDollar as DollarIcon,
  ListChecks as TasksIcon,
  Circle as CircleIcon,
  CircleNotch as SpinnerIcon,
} from "@phosphor-icons/react";
