import { useT } from '@/i18n/useT'

interface ImagePickerProps {
  onPick?: (file: File) => void
}

export function ImagePicker({ onPick }: ImagePickerProps) {
  const t = useT('voice')

  return (
    <label className="terra-image-picker">
      <span>{t('image.pick', 'Camera or gallery')}</span>
      <input
        type="file"
        accept="image/*"
        capture="environment"
        onChange={event => {
          const file = event.target.files?.[0]
          if (file) onPick?.(file)
        }}
      />
    </label>
  )
}

export default ImagePicker
