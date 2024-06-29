# Defines the function Sum with two parameters: start and target
def Sum(start, target):
  if start == target:
    # If the value of start is the same as target, returns start.
    return start
  else:
    # If start is not equal to target, recursively call Sum with
    # start incremented by 1, and add the result to start.
    return start + Sum(start + 1, target)  

def main():
  # This translates to (1 + (2 + (3 + (...... + (999999 + 1000000)))))
  # Note that this will overflow the maximum value of a number in Bend
  return Sum(1, 1_000_000)