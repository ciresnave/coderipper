# SPDX-License-Identifier: MIT OR Apache-2.0
"""OCR a scanned two-column patent PDF page range (used for the US12645455B2 check, 2026-11-16).

Setup (Python 3.14 venv; 3.15 has no onnxruntime wheel):
    pip install pypdfium2 pillow rapidocr-onnxruntime
Fetch the USPTO PDF:  https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/<number>
Usage: python ocr-scanned-pdf.py <pdf> <first-page> <last-page> <out.txt>
Renders each page at 3x, OCRs left and right columns separately, and saves the page
crops next to <out.txt> so the claim text can be checked against the images by eye.
Never trust OCR alone: read the saved images for any claim you quote.
Third-party tools imported (licences from their installed package metadata, 2026-11-16):
    pypdfium2 (BSD-3-Clause OR Apache-2.0; bundles PDFium, BSD-3-Clause)
    rapidocr-onnxruntime (Apache-2.0; pulls in onnxruntime, MIT)
    pillow (MIT-CMU)
Windows' built-in OCR via powershell.exe 5.1 hung on this box; do not retry it.
"""
import sys, pypdfium2 as pdfium
from rapidocr_onnxruntime import RapidOCR
pdf=pdfium.PdfDocument(sys.argv[1]); a,b=int(sys.argv[2]),int(sys.argv[3])
ocr=RapidOCR(); out=open(sys.argv[4],'w',encoding='utf8')
for i in range(a,b+1):
    img=pdf[i-1].render(scale=3).to_pil(); w,h=img.size
    out.write(f"=====PAGE {i}=====\n")
    for name,box in (("L",(0,0,w//2,h)),("R",(w//2,0,w,h))):
        p=f"{sys.argv[4]}.p{i}{name}.png"; img.crop(box).save(p)
        res,_=ocr(p); res=sorted(res or [],key=lambda r:r[0][0][1])
        out.write(f"--- {name} ---\n"+"\n".join(r[1] for r in res)+"\n")
    out.flush()
