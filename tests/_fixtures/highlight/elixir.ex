defmodule Greeter do
  @moduledoc "Says hello."

  def greet(name) when is_binary(name) do
    "Hello, #{name}!"
  end
end
