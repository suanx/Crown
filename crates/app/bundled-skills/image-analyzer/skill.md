---
name: image-analyzer
description: 图片分析与处理 — OCR 文字识别、图像信息提取、格式转换、批量处理、截图对比。
metadata:
  display-name: 图片分析
  icon: "🖼️"
  usage: "发送图片或在对话中描述图片处理需求"
---

# 图片分析指令

协助用户处理各类图片相关任务。

## 可用工具

### OCR 文字识别

```bash
# 方法一：pytesseract（推荐，支持中文）
pip install pytesseract pillow -q 2>/dev/null
python3 -c "
import pytesseract, sys
from PIL import Image
img = Image.open('$FILE')
text = pytesseract.image_to_string(img, lang='chi_sim+eng')
print(text)
"

# 方法二：easyocr（更准确，但首次较慢）
pip install easyocr -q 2>/dev/null
python3 -c "
import easyocr
reader = easyocr.Reader(['ch_sim', 'en'], gpu=False)
results = reader.readtext('$FILE')
for bbox, text, conf in results:
    print(f'[{conf:.2f}] {text}')
"
```

### 图片信息提取

```bash
python3 -c "
from PIL import Image
from PIL.ExifTags import TAGS
img = Image.open('$FILE')
print(f'尺寸: {img.size}')
print(f'格式: {img.format}')
print(f'模式: {img.mode}')
exif = img._getexif()
if exif:
    for k, v in exif.items():
        name = TAGS.get(k, k)
        print(f'{name}: {v}')
"
```

### 格式转换

```bash
# 批量转换 PNG → JPG
python3 -c "
from PIL import Image
import glob, os
for f in glob.glob('*.png'):
    img = Image.open(f).convert('RGB')
    img.save(f.replace('.png', '.jpg'), 'JPEG', quality=85)
    print(f'转换: {f}')
"
```

### 截图对比

```bash
# 像素级对比
python3 -c "
from PIL import Image, ImageChops
img1 = Image.open('before.png')
img2 = Image.open('after.png')
diff = ImageChops.difference(img1, img2)
if diff.getbbox():
    diff.save('diff.png')
    print(f'差异区域: {diff.getbbox()}')
    changed = sum(1 for p in diff.getdata() if any(c > 10 for c in p))
    print(f'变化像素: {changed}/{img1.size[0]*img1.size[1]}')
else:
    print('图片完全相同')
"
```

## 规则

- 优先用 Python 生态，跨平台兼容
- OCR 结果可能有误差，标注置信度
- 批量处理前先测试单个文件
- 大图片（>10MB）先压缩再处理
