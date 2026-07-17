import os
from PIL import Image

def pad_to_square(img_path, out_path, size=512):
    img = Image.open(img_path).convert("RGBA")
    
    # Crop to the actual visible (non-transparent) pixels
    bbox = img.getbbox()
    if bbox:
        img = img.crop(bbox)
        
    w, h = img.size
    
    # Add a little padding (10%) so it doesn't touch the very edges of the icon
    padding = int(max(w, h) * 0.1)
    w_padded = w + padding * 2
    h_padded = h + padding * 2
    
    max_dim = max(w_padded, h_padded)
    
    # Create new transparent square image
    new_img = Image.new('RGBA', (max_dim, max_dim), (0, 0, 0, 0))
    
    # Paste cropped image in the center
    paste_x = (max_dim - w) // 2
    paste_y = (max_dim - h) // 2
    new_img.paste(img, (paste_x, paste_y))
    
    # Resize to exactly 512x512 smoothly
    new_img = new_img.resize((size, size), Image.Resampling.LANCZOS)
    new_img.save(out_path)
    print("Saved cropped and padded icon to", out_path)

if __name__ == "__main__":
    pad_to_square("src/assets/logo.png", "public/icon-base.png")
