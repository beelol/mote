# profile

What a person needs when they ask "who is this" or "show my profile." An identity header, a few
stats, and the detail fields.

## Make it

```
screen "Profile" {
  col center {
    avatar profile.person.avatar alt profile.person.name
    title profile.person.name
    caption profile.person.handle
  }
  row {
    stat "Repos" profile.person.repos
    stat "Followers" profile.person.followers
    stat "Following" profile.person.following
  }
  section "Details" {
    kv "Email" profile.person.email
    kv "Location" profile.person.location
    timestamp profile.person.joined
  }
  footer { button "Edit" primary -> EditProfile }
}
```

## Slice

- `profile.person` is a `Value<Person>` with `name: str`, `handle: str`, `avatar: asset`,
  `repos: number`, `followers: number`, `following: number`, `email: str`, `location: str`,
  `joined: datetime`.

## Free

- The avatar loads with a placeholder, stats format, the layout centers the identity block.

## Animates

- Stat numbers roll to their values when the profile loads or updates.
