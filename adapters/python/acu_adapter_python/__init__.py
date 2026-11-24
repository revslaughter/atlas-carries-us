"""The Python adapter of Atlas Carries Us.

It reads Python code with the standard library's ``ast`` module, never
imports or runs it, and writes the data format of docs/schema.md. See
docs/adapters.md, section "Python", for the decisions and the limits.
"""
