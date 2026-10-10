# profile

What a person needs when they ask "who is this" or "show my profile." An identity header, a few
stats, and the detail fields.

## Make it

```
screen "Profile" {
  col center {
    avatar .avatar alt .name
    title .name
    caption .handle
  }
  row {
    stat "Repos" .repos
    stat "Followers" .followers
    stat "Following" .following
  }
  section "Details" {
    kv "Email" .email
    kv "Location" .location
    kv "Joined" .joined
  }
  footer { button "Edit" primary -> EditProfile }
}
```

## Slice

- The profile is a `Value<Person>` with `name: str`, `handle: str`, `avatar: asset`, `repos`,
  `followers`, `following`, `email`, `location`, `joined`.

## Free

- The avatar loads with a placeholder, stats format, the layout centers the identity block.

## Animates

- Stat numbers roll to their values when the profile loads or updates.
