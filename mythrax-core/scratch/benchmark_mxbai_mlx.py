import time
import mlx.core as mx
import mlx.nn as nn

print("MLX Default Device:", mx.default_device())

class Qwen2Attention(nn.Module):
    def __init__(self):
        super().__init__()
        self.q_proj = nn.Linear(1536, 1536, bias=True)
        self.k_proj = nn.Linear(1536, 512, bias=True)
        self.v_proj = nn.Linear(1536, 512, bias=True)
        self.o_proj = nn.Linear(1536, 1536, bias=True)

    def __call__(self, x, mask):
        q = self.q_proj(x)
        k = self.k_proj(x)
        v = self.v_proj(x)
        
        B, L, _ = q.shape
        q = q.reshape(B, L, 12, 128).transpose(0, 2, 1, 3)
        k = k.reshape(B, L, 4, 128).transpose(0, 2, 1, 3)
        v = v.reshape(B, L, 4, 128).transpose(0, 2, 1, 3)
        
        k = mx.repeat(k, 3, axis=1)
        v = mx.repeat(v, 3, axis=1)
        
        scale = 1.0 / (128 ** 0.5)
        out = mx.fast.scaled_dot_product_attention(q, k, v, scale=scale, mask=mask)
        out = out.transpose(0, 2, 1, 3).reshape(B, L, -1)
        return self.o_proj(out)

class Qwen2DecoderLayer(nn.Module):
    def __init__(self):
        super().__init__()
        self.input_layernorm = nn.RMSNorm(1536)
        self.self_attn = Qwen2Attention()
        self.post_attention_layernorm = nn.RMSNorm(1536)
        self.down_proj = nn.Linear(1536 * 2, 1536, bias=False)

    def __call__(self, x, mask):
        h = self.self_attn(self.input_layernorm(x), mask)
        x = x + h
        h2 = self.post_attention_layernorm(x)
        h2 = mx.concatenate([h2, h2], axis=-1)
        x = x + self.down_proj(h2)
        return x

class MockQwen2(nn.Module):
    def __init__(self):
        super().__init__()
        self.embed = nn.Embedding(151646, 1536)
        self.layers = [Qwen2DecoderLayer() for _ in range(28)]
        self.norm = nn.RMSNorm(1536)

    def __call__(self, ids, mask):
        x = self.embed(ids)
        for layer in self.layers:
            x = layer(x, mask)
        return self.norm(x)

model = MockQwen2()
mx.eval(model.parameters())

for batch in [1, 2, 4, 8, 16]:
    ids = mx.random.randint(0, 151646, (batch, 384))
    mask = mx.zeros((batch, 1, 384, 384))
    # Warmup
    out = model(ids, mask)
    mx.eval(out)
    
    # Bench
    start = time.time()
    for _ in range(5):
        out = model(ids, mask)
        mx.eval(out)
    end = time.time()
    print(f"Batch Size {batch} - Latency: {(end - start) / 5 * 1000:.2f} ms")
