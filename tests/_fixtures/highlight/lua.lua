local function greet(name)
  return "Hello, " .. name
end

for _, name in ipairs({ "ada", "linus" }) do
  print(greet(name))
end
