import os
from PIL import Image

def pad_to_square(img_path, out_path, size=512):
    img = Image.open(img_path).convert("RGBA")
    
    # Calculate aspect ratio
    w, h = img.size
    
    # We want to scale it so the maximum dimension fits in `size` while keeping aspect ratio.
    # But wait, if it's "wide and short", it's distorted. Maybe `tauri icon` stretched it.
    # Let's create a perfectly square canvas (transparent) and paste the image in the center.
    max_dim = max(w, h)
    
    # Create new transparent square image
    new_img = Image.new('RGBA', (max_dim, max_dim), (0, 0, 0, 0))
    
    # Paste original image in the center
    paste_x = (max_dim - w) // 2
    paste_y = (max_dim - h) // 2
    new_img.paste(img, (paste_x, paste_y))
    
    # Resize to exactly 512x512 smoothly
    new_img = new_img.resize((size, size), Image.Resampling.LANCZOS)
    new_img.save(out_path)
    print("Saved padded icon to", out_path)

if __name__ == "__main__":
    pad_to_square("src/assets/logo.png", "public/icon-base.png")
