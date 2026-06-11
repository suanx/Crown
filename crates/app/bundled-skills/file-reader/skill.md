---
name: file-reader
description: 读取 PDF、Word、Excel、PPT 等办公文档格式，将其转换为 Markdown 文本供模型分析。支持文本提取、表格解析、图文混排内容。
metadata:
  display-name: 文档读取
  icon: "📄"
  usage: "在对话中直接发送文件或要求读取某文件即可自动调用"
---

# 办公文档读取指令

你已启用文件读取能力。当用户请求读取 PDF、Word、Excel、PPT 等文档时，按以下方法处理。

## 可用工具与方法

根据文件类型选择最合适的方法：

### PDF 文件 (.pdf)

**方法一：Python PDF 解析（推荐，最准确）**
```bash
# 使用 pdfplumber（文本+表格提取，安装快）
pip install pdfplumber -q 2>/dev/null && python3 -c "
import pdfplumber, sys
with pdfplumber.open('$FILE') as pdf:
    for i, page in enumerate(pdf.pages):
        print(f'## 第 {i+1} 页')
        text = page.extract_text()
        if text:
            print(text)
        tables = page.extract_tables()
        for j, t in enumerate(tables):
            print(f'\\n### 表格 {j+1}')
            for row in t:
                print('| ' + ' | '.join(str(c or '') for c in row) + ' |')
"
```

**方法二：PyMuPDF (fitz) — 更快，含图片元数据**
```bash
pip install pymupdf -q 2>/dev/null && python3 -c "
import fitz
doc = fitz.open('$FILE')
for i, page in enumerate(doc):
    print(f'## 第 {i+1} 页')
    print(page.get_text())
    # 如需提取图片：
    # for img in page.get_images():
    #     xref = img[0]; pix = fitz.Pixmap(doc, xref)
    #     pix.save(f'page{i+1}_img{xref}.png')
"
```

**方法三：pdftotext（系统工具，零依赖）**
```bash
pdftotext "$FILE" - 2>/dev/null || mutool draw -F text "$FILE" 2>/dev/null
```

**提取表格（CSV 格式）**
```bash
pip install camelot-py -q 2>/dev/null && python3 -c "
import camelot
tables = camelot.read_pdf('$FILE')
for i, t in enumerate(tables):
    t.to_csv(f'table_{i}.csv')
    print(f'表格 {i+1}: {len(t)} 行×{len(t.columns)} 列')
    print(t.df.to_string())
"
```

### Word 文档 (.docx)

```bash
pip install python-docx -q 2>/dev/null && python3 -c "
from docx import Document
doc = Document('$FILE')
print('# 文档内容\\n')
for p in doc.paragraphs:
    if p.text.strip():
        print(p.text + '\\n')
if doc.tables:
    print('\\n## 表格')
    for t in doc.tables:
        for row in t.rows:
            print('| ' + ' | '.join(c.text for c in row.cells) + ' |')
        print()
"
```

### Excel 文件 (.xlsx / .xls)

```bash
pip install openpyxl -q 2>/dev/null && python3 -c "
import openpyxl
wb = openpyxl.load_workbook('$FILE', data_only=True)
print(f'# 工作簿 ({len(wb.sheetnames)} 个工作表)')
print(f'工作表列表: {\", \".join(wb.sheetnames)}\\n')
for name in wb.sheetnames:
    ws = wb[name]
    print(f'## 工作表: {name} ({ws.max_row} 行 × {ws.max_column} 列)\\n')
    for i, row in enumerate(ws.iter_rows(values_only=True), 1):
        print('| ' + ' | '.join(str(c or '') for c in row) + ' |')
        if i > 200:  # 防止超长输出
            print(f'| ... (已截断, 共 {ws.max_row} 行) |')
            break
    print()
" 2>&1 || python3 -c "
import xlrd
wb = xlrd.open_workbook('$FILE')
for name in wb.sheet_names():
    ws = wb.sheet_by_name(name)
    print(f'## 工作表: {name} ({ws.nrows} 行 × {ws.ncols} 列)\\n')
    for i in range(min(ws.nrows, 200)):
        print('| ' + ' | '.join(str(ws.cell_value(i, j) or '') for j in range(ws.ncols)) + ' |')
        print()
"
```

### PPT 文件 (.pptx)

```bash
pip install python-pptx -q 2>/dev/null && python3 -c "
from pptx import Presentation
prs = Presentation('$FILE')
for i, slide in enumerate(prs.slides, 1):
    print(f'## 幻灯片 {i}')
    for shape in slide.shapes:
        if shape.has_text_frame:
            for p in shape.text_frame.paragraphs:
                if p.text.strip():
                    print(p.text)
        if shape.has_table:
            t = shape.table
            for row in t.rows:
                print('| ' + ' | '.join(c.text for c in row.cells) + ' |')
            print()
    print()
"
```

### CSV / TSV / 纯文本

```bash
# CSV 文件 — 用 python 确保正确处理编码
python3 -c "
import csv, sys
with open('$FILE', 'r', encoding='utf-8-sig') as f:
    reader = csv.reader(f)
    for i, row in enumerate(reader):
        print('| ' + ' | '.join(row) + ' |')
        if i == 0 and len(row) < 3:
            break  # 简短文件
    # 统计行数列数
    f.seek(0)
    rows = list(csv.reader(f))
    print(f'\n*统计: {len(rows)} 行, {len(rows[0]) if rows else 0} 列*')
" 2>/dev/null || head -500 "$FILE"
```

## 通用规则

1. **优先使用 Python**：Python 生态对各类文档格式支持最全面、跨平台一致
2. **安装依赖**：首次使用时 pip install 对应库（安装后缓存，后续不再等待）
3. **处理编码**：中文文件可能出现 GBK/GB2312，尝试 `encoding='utf-8-sig'` 和 `encoding='gbk'`
4. **截断保护**：输出超过 50000 字符时自动截断并提示，避免超出上下文限制
5. **图片跳过**：文档中的图片无法读取文本内容时，在回复中注明「此页含 N 张图片」
6. **扫描件特殊处理**：如果是图片扫描件的 PDF，告知用户需要 OCR（可用 `pytesseract`）
7. **密码保护**：文件有密码保护时，告知用户需要先解除密码
8. **大文件**：超过 100 页或 50MB 的文件，先获取文件大小和页数，再分段处理
9. **始终先检查文件是否存在、大小、是否可读**

## 注意事项

- 这些方法通过 `run_command` 工具执行，所有输出流式返回到对话中
- 提取结果会以文本形式呈现，模型可以直接分析内容
- 表格数据保留 Markdown 表格格式，便于阅读和后续处理
- Excel 多工作表会逐一展示，PPT 每张幻灯片逐项呈现
- 所有临时安装的 Python 包仅限当前环境，不影响系统
