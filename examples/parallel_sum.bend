# Defines the function Sum with two parameters: start and target
def Sum(start, target):
  if start == target:
    # If the value of start is the same as target, returns start.
    return start
  else:
    # If start is not equal to target, calculate the midpoint (half),
    # then recursively call Sum on both halves.
    half = (start + target) / 2
    left = Sum(start, half)  # (Start -> Half)
    right = Sum(half + 1, target)
    return left + right

# A parallelizable sum of numbers from 1 to 1000000
def main():
  # This translates to (((1 + 2) + (3 + 4)) + ... (999999 + 1000000)...)
  return Sum(1, 1_000_000)