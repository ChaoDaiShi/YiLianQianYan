# Python 测试参考指南

## pytest 基本用法

```python
# test_module.py
import pytest

def test_simple():
    assert 1 + 1 == 2

@pytest.mark.parametrize("input,expected", [
    (1, 2), (2, 4), (3, 6)
])
def test_double(input, expected):
    assert input * 2 == expected
```

## Mock 和 Fixture

```python
@pytest.fixture
def sample_data():
    return {"name": "test", "value": 42}

def test_with_fixture(sample_data):
    assert sample_data["value"] == 42
```

## 覆盖率

```bash
pytest --cov=src --cov-report=html
```
