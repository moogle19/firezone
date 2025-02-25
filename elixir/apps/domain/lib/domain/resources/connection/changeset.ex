defmodule Domain.Resources.Connection.Changeset do
  use Domain, :changeset

  @fields ~w[gateway_group_id]a
  @required_fields @fields

  def changeset(account_id, connection, attrs) do
    connection
    |> cast(attrs, @fields)
    |> validate_required(@required_fields)
    |> assoc_constraint(:resource)
    |> assoc_constraint(:gateway_group)
    |> assoc_constraint(:account)
    |> put_change(:account_id, account_id)
  end
end
